//! Control how replays are distributed or stored.
//!
//! This plugin is defaultly enabled.

use std::io::Cursor;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use binrw::BinWrite;
use bytes::Bytes;
use chrono::Local;
use linkme::distributed_slice;
use log::warn;
use tokio::sync::broadcast;

use ygopro_data::complex::Complex;
use ygopro_data::constants::Mode;
use ygopro_data::data::ReplayMode;
use ygopro_data::message::ctos;
use ygopro_data::message::gm;
use ygopro_data::message::gm::MaskedClone;
use ygopro_data::message::stoc;
use ygopro_derive::Attachment;
use ygopro_derive::Configuration;
use ygopro_derive::after;
use ygopro_derive::handler;
use ygopro_derive::register_to;
use ygopro_handler::MessageKey;

use crate::duel::Duel;
use crate::duel::SendTarget;
use crate::message as ygopro;
use crate::ygopro_handlers::Handler;
use crate::ygopro_handlers::HandlerEx;
use crate::ygopro_handlers::YGOPRO_HANDLERS;
use crate::ygopro_handlers::YGOPRO_HANDLERS_EX;

#[distributed_slice(crate::plugin::DEFAULT_ENABLED_PLUGINS)]
pub static NAME: &'static str = module_path!();

/// The serialized replay format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    /// The ygopro `.yrp` replay format.
    #[default]
    Ygopro,
    /// The ygopro2 (forge) `.yrp3d` replay format.
    #[cfg(feature = "forge")]
    YgoproForge,
    /// The srvpro `.bin` replay format.
    Srvpro,
}

impl FromStr for Format {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "ygopro" => Ok(Format::Ygopro),
            #[cfg(feature = "forge")]
            "forge" => Ok(Format::YgoproForge),
            "srvpro" => Ok(Format::Srvpro),
            _ => Err(())
        }
    }
}

#[derive(Clone, Configuration)]
pub struct Configuration {
    #[config(not_from_env, default = "ReplayMode::empty()")]
    pub mode: ReplayMode,
    #[config(default = "\"replays\".to_string()")]
    pub save_path: String,
    pub format: Format,
    #[config(default = "\"{time} {players} Duel {duel}\".to_string()")]
    pub file_template: String,
    #[config(default = "\"%Y-%m-%d %H-%M-%S\".to_string()")]
    pub time_format: String,
    #[config(default = "0")]
    pub machine: u16,
}

#[derive(Clone)]
pub struct ReplayWithName {
    pub data: Bytes,
    pub name: String,
    pub id: String,
    pub path: Option<PathBuf>,
}

#[derive(Attachment)]
pub struct Attachment {
    #[attachment(default = "create_sender()")]
    sender: broadcast::Sender<ReplayWithName>,
}

impl Deref for Attachment {
    type Target = broadcast::Sender<ReplayWithName>;

    fn deref(&self) -> &Self::Target {
        &self.sender
    }
}

fn create_sender() -> broadcast::Sender<ReplayWithName> {
    let (sender, _receiver) = broadcast::channel(16);
    sender
}

mod unique_id {
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    const EPOCH: u64 = 1_600_000_000_000;
    const MACHINE_BITS: u64 = 10;
    const SEQUENCE_BITS: u64 = 12;
    const MACHINE_MASK: u64 = (1 << MACHINE_BITS) - 1;
    const SEQUENCE_MASK: u64 = (1 << SEQUENCE_BITS) - 1;

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    pub fn digest(machine: u16) -> String {
        let mut id = snowflake(machine);
        let mut characters = Vec::new();
        loop {
            characters.push(char::from_digit((id % 36) as u32, 36).unwrap_or('0'));
            id /= 36;
            if id == 0 { break }
        }
        characters.into_iter().rev().collect()
    }

    fn snowflake(machine: u16) -> u64 {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis() as u64).unwrap_or(0).saturating_sub(EPOCH);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed) & SEQUENCE_MASK;
        (timestamp << (MACHINE_BITS + SEQUENCE_BITS)) | ((machine as u64 & MACHINE_MASK) << SEQUENCE_BITS) | sequence
    }
}

fn render_replay_name(duel: &Duel, template: &str, time_format: &str, id: &str) -> String {
    let time = Local::now().format(time_format).to_string();
    let name = template
        .replace("{time}", &time)
        .replace("{players}", &render_players_string(duel))
        .replace("{duel}", &duel.duel_count.to_string())
        .replace("{id}", id);
    name.replace(['/', '\\', '?', '*'], "_")
}

fn render_players_string(duel: &Duel) -> String {
    let names = duel.players.iter().flatten().map(|player| &*player.name).collect::<Vec<&str>>();
    if duel.host_info.mode == Mode::Tag && names.len() == 4 {
        format!("{} & {} VS {} & {}", names[0], names[1], names[2], names[3])
    } else {
        names.join(" VS ")
    }
}

fn generate_replay_binary(format: Format, include_chat: bool, duel: &Duel, replay: Option<&stoc::Replay>) -> Option<(&'static str, Vec<u8>)> {
    match format {
        Format::Ygopro => {
            if include_chat {
                warn!("ygopro replay format cannot carry chat, the chat is dropped.")
            }
            Some(("yrp", serialize(&replay?.replay)?))
        }
        #[cfg(feature = "forge")]
        Format::YgoproForge => {
            let mut forge_replay = duel.create_replay_forge(false)?;
            if let Some(replay) = replay {
                forge_replay.messages.push(gm::SibylReplay { replay: replay.replay.clone() }.into());
            }
            if !include_chat {
                forge_replay.messages.retain(|message| !matches!(message, gm::Message::SibylChat(_)));
            }
            Some(("yrp3d", serialize(&forge_replay)?))
        }
        Format::Srvpro => {
            let directed = [
                stoc::MessageType::JoinGame.message_key(),
                stoc::MessageType::SelectTp.message_key(),
                stoc::MessageType::SelectHand.message_key(),
                stoc::MessageType::TimeLimit.message_key(),
                stoc::MessageType::ChangeSide.message_key(),
                stoc::MessageType::HandResult.message_key(),
                stoc::MessageType::TeammateSurrender.message_key(),
            ];
            let messages = duel.sender.messages.iter()
                .filter_map(|message| {
                    let recorded = message.try_get().ok()?;
                    if directed.contains(&recorded.message_key()) { return None }
                    match recorded {
                        stoc::Message::GameMessage(game_message) => match &game_message.message {
                            gm::Message::Start(start) if start.player_type & 0xf0 == 0 => None,
                            _ => Some(Complex::from_message(stoc::Message::from(game_message.message.clone_masked()))),
                        },
                        stoc::Message::Chat(_) if !include_chat => None,
                        _ => Some(message.clone()),
                    }
                })
                .collect::<Vec<Complex<stoc::Message>>>();
            let mut cursor = Cursor::new(Vec::new());
            for message in &messages {
                let length = u16::try_from(message.bytes().len()).ok()?;
                length.write_le(&mut cursor).ok()?;
                message.write_le(&mut cursor).ok()?;
            }
            Some(("bin", cursor.into_inner()))
        }
    }
}

fn serialize<Message: BinWrite>(message: &Message) -> Option<Vec<u8>> where for<'a> <Message as BinWrite>::Args<'a>: Default {
    let mut cursor = Cursor::new(Vec::new());
    message.write_le(&mut cursor).ok()?;
    Some(cursor.into_inner())
}

#[after(ctos::CreateGame)]
#[register_to(YGOPRO_HANDLERS as Handler)]
fn on_create_game(_attachment: &mut Attachment) { }

#[after(ygopro::GenerateReplay)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_generate_replay(configuration: Configuration, target: &mut SendTarget) {
    if configuration.mode.contains(ReplayMode::WatcherNoSend) {
        *target = SendTarget::AllPlayer
    }
}

#[handler(ygopro::GenerateReplay, priority = 200)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_save_replay(duel: &mut Duel, attachment: &mut Attachment, configuration: Configuration, states: &mut anymap3::Map<dyn std::any::Any + Send>) {
    let Some((extension, data)) = generate_replay_binary(configuration.format, configuration.mode.contains(ReplayMode::IncludeChat), duel, states.get::<stoc::Replay>()) else { return };
    let id = unique_id::digest(configuration.machine);
    let name = render_replay_name(duel, &configuration.file_template, &configuration.time_format, &id);
    let mut saved_path = None;
    if configuration.mode.contains(ReplayMode::SaveInServer) && !configuration.file_template.is_empty() {
        let path = Path::new(&configuration.save_path).join(format!("{name}.{extension}"));
        if let Some(directory) = path.parent() && let Err(error) = std::fs::create_dir_all(directory) {
            warn!("failed to create replay directory {}: {error}", directory.display());
        } else if let Err(error) = std::fs::write(&path, &data) {
            warn!("failed to save replay {}: {error}", path.display());
        } else {
            saved_path = Some(path);
        }
    }
    if attachment.receiver_count() > 0 {
        attachment.send(ReplayWithName { data: data.into(), name, id, path: saved_path }).ok();
    }
}

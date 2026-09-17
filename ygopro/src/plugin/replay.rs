//! Control how replays are distributed or stored.
//!
//! This plugin is defaultly enabled.
//!
//! # Examples
//!
//! Enable the module with a replay mode:
//!
//! ```
//! use ygopro::plugin::replay::Configuration;
//! use ygopro::plugin::replay::Format;
//!
//! let mut configuration = ygopro::Configuration::default();
//! configuration.enable_plugin_with_configuration(
//!     "ygopro::plugin::replay",
//!     Configuration {
//!         mode: ygopro_data::data::ReplayMode::WatcherNoSend,
//!         save_path: "replays".to_string(),
//!         file_template: "%Y-%m-%d %H-%M-%S {players}".to_string(),
//!         format: Format::Ygopro,
//!     },
//! );
//! ```

use std::io::Cursor;
use std::path::Path;
use std::str::FromStr;

use binrw::BinWrite;
use chrono::Local;
use linkme::distributed_slice;
use log::warn;

use ygopro_data::constants::Mode;
use ygopro_data::data::ReplayMode;
#[cfg(feature = "forge")]
use ygopro_data::message::gm;
use ygopro_derive::Configuration;
use ygopro_derive::after;
use ygopro_derive::register_to;

use crate::duel::Duel;
use crate::duel::SendTarget;
use crate::message as ygopro;
use crate::ygopro_handlers::HandlerEx;
use crate::ygopro_handlers::YGOPRO_HANDLERS_EX;

/// Name for activitating this module in the plugin system.
#[distributed_slice(crate::plugin::DEFAULT_ENABLED_PLUGINS)]
pub static NAME: &'static str = module_path!();

/// Replay distribution or storage policy.
#[derive(Clone, Configuration)]
pub struct Configuration {
    /// Mode controlling how replays are distributed or stored.
    #[config(not_from_env, default = "ReplayMode::empty()")]
    pub mode: ReplayMode,
    #[config(default = "\"replays\".to_string()")]
    pub save_path: String,
    #[config(default = "\"%Y-%m-%d %H-%M-%S {players}\".to_string()")]
    pub file_template: String,
    pub format: Format,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Format {
    Ygopro,
    #[cfg(feature = "forge")]
    YgoproForge,
    Srvpro,
    #[default]
    Raw
}

impl FromStr for Format {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "ygopro" => Ok(Format::Ygopro),
            #[cfg(feature = "forge")]
            "forge" => Ok(Format::YgoproForge),
            "srvpro" => Ok(Format::Srvpro),
            "raw" => Ok(Format::Raw),
            _ => Err(())
        }
    }
}

#[after(ygopro::GenerateReplay)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_generate_replay(configuration: Configuration, target: &mut SendTarget) {
    if configuration.mode.contains(ReplayMode::WatcherNoSend) {
        *target = SendTarget::AllPlayer
    }
}

#[after(ygopro::GenerateReplay)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_save_replay(duel: &mut Duel, configuration: Configuration) {
    if !configuration.mode.contains(ReplayMode::SaveInServer) { return }

    let Some((extension, data)) = serialize(&configuration, duel) else { return };

    let file_name = file_name(&configuration, duel);
    let path = Path::new(&configuration.save_path).join(format!("{file_name}.{extension}"));
    if let Some(directory) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(directory) {
            warn!("failed to create replay directory {}: {error}", directory.display());
            return;
        }
    }
    if let Err(error) = std::fs::write(&path, data) {
        warn!("failed to save replay {}: {error}", path.display());
    }
}

fn serialize(configuration: &Configuration, duel: &Duel) -> Option<(&'static str, Vec<u8>)> {
    match configuration.format {
        Format::Ygopro => {
            if configuration.mode.contains(ReplayMode::IncludeChat) {
                warn!("ygopro replay format cannot carry chat, the chat is dropped.")
            }
            Some(("yrp", to_bytes(&duel.create_replay()?)?))
        }
        #[cfg(feature = "forge")]
        Format::YgoproForge => {
            let mut replay = duel.create_replay_forge(false)?;
            if !configuration.mode.contains(ReplayMode::IncludeChat) {
                replay.messages.retain(|message| !matches!(message, gm::Message::SibylChat(_)));
            }
            Some(("yrp3d", to_bytes(&replay)?))
        }
        Format::Srvpro => {
            warn!("srvpro replay format is not supported for now.");
            None
        }
        Format::Raw => {
            warn!("raw replay format is not supported for now.");
            None
        }
    }
}

fn to_bytes<Message: BinWrite>(message: &Message) -> Option<Vec<u8>> where for<'a> <Message as BinWrite>::Args<'a>: Default {
    let mut cursor = Cursor::new(Vec::new());
    message.write_le(&mut cursor).ok()?;
    Some(cursor.into_inner())
}

fn file_name(configuration: &Configuration, duel: &Duel) -> String {
    let name = Local::now().format(&configuration.file_template).to_string();
    let name = name.replace("{players}", &players(duel));
    name.replace(['/', '\\', '?', '*'], "_")
}

fn players(duel: &Duel) -> String {
    let names = duel.players.iter().flatten().map(|player| &*player.name).collect::<Vec<&str>>();
    if duel.host_info.mode == Mode::Tag && names.len() == 4 {
        format!("{} & {} VS {} & {}", names[0], names[1], names[2], names[3])
    } else {
        names.join(" VS ")
    }
}

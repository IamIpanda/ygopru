//! Allow join duel on middle of duel.
//! 
//! When that happened, server will accept user as observer, and send all history message to that user.
//! 
//! # Notes
//!
//! `masked_messages` is a field of the duel and must not be changed. It already carries enough
//! information to rebuild the duel state, so no reconstruction is needed. What has to be filtered
//! out are the messages an observer never receives:
//!
//! - `SelectHand` / `SelectTp` / `HandResult`: a duelist picks a hand or a turn.
//! - `ChangeSide`: switching the side deck, which is not replayed.
//! - `DeckCount`: sent to each duelist separately with the self/opponent counts swapped, while the
//!   observer copy of `gm::Start` already carries both deck counts.
//! - `JoinGame`: the observer receives its own join response.
//! - `TypeChange`: the copy in `masked_messages` has lost its parsed message, so the observer index
//!   degrades to `Observer(255)` when it is read back, and replaying it would overwrite the
//!   position that the provider bridge learned from the stoc stream (see `crate::host`).
//! - `LeaveGame`: only sent to the client being removed.
//! - `TimeLimit`: directed at the duelist whose turn timer is running.
//! - `TeammateSurrender`: specific to a tag-duel team player.
//!
//! Also skipped:
//!
//! - every `gm::Start` that is not the observer copy: the duel sends one copy per duelist plus one
//!   for all observers, and the client rebuilds the field on every copy it receives.
//! - any `waiting_for` game message: an observer must not be prompted to choose.
//!
//! # Examples
//!
//! Enable the module:
//!
//! ```
//! let mut configuration = ygopro::Configuration::default();
//! configuration.enable_plugin("ygopro::plugin::soumatou");
//! ```

use std::any::Any;
use std::ops::Deref;

use log::warn;

use ygopro_data::constants::DuelStage;
use ygopro_data::constants::Netplayer;
use ygopro_data::message::{ctos, stoc, gm};
use ygopro_data::message::gm::GameMessage;

use ygopro_derive::after;
use ygopro_derive::command;
use ygopro_derive::register_to;
use ygopro_handler::MessageKey;

use crate::duel::Duel;
use crate::message as ygopro;
use crate::ygopro_handlers::Handler;
use crate::ygopro_handlers::HandlerEx;
use crate::ygopro_handlers::Response;
use crate::ygopro_handlers::YGOPRO_HANDLERS;
use crate::ygopro_handlers::YGOPRO_HANDLERS_EX;

/// Name for activitating this module in the plugin system.
#[linkme::distributed_slice(crate::plugin::DEFAULT_ENABLED_PLUGINS)]
pub static NAME: &'static str = module_path!();

#[command]
#[register_to(crate::command::COMMANDS as crate::command::CommandHandler with &'static str)]
fn soumatou(duel: &mut Duel, arguments: &mut Box<dyn Any + Send>) {
    let Some(target) = arguments.downcast_ref::<Netplayer>().copied() else {
        return;
    };
    let duel_start_key = stoc::DuelStart.message_key();
    let directed_keys = [
        stoc::MessageType::SelectHand.message_key(),
        stoc::MessageType::SelectTp.message_key(),
        stoc::MessageType::HandResult.message_key(),
        stoc::MessageType::ChangeSide.message_key(),
        stoc::MessageType::DeckCount.message_key(),
        stoc::MessageType::JoinGame.message_key(),
        stoc::MessageType::TypeChange.message_key(),
        stoc::MessageType::LeaveGame.message_key(),
        stoc::MessageType::TimeLimit.message_key(),
        stoc::MessageType::TeammateSurrender.message_key(),
    ];
    for message in duel.sender.masked_messages.iter().skip_while(|message| message.message_key() != duel_start_key) {
        if directed_keys.contains(&message.message_key()) { continue; }
        if let stoc::Message::GameMessage(game_message) = message.deref() {
            if game_message.message.waiting_for().is_some() {
                continue;
            }
            if let gm::Message::Start(start) = &game_message.message && !start.player_type.is_observer() {
                continue;
            }
        }
        duel.sender.send_without_record(message.clone(), target.into());
    }
}

#[after(ctos::JoinGame)]
#[register_to(YGOPRO_HANDLERS)]
fn on_join(duel: &mut Duel, player: Netplayer) {
    if duel.stage > DuelStage::Begin {
        if let Netplayer::Observer(index) = player {
            // we cannot directly send all messages here becuase
            // we need to wait messages typechange, playerenter, etc
            // which is in current result send to user.
            // so the only way is to make that a command and that
            // will queue after the current response messages.
            duel.request_sender.send(crate::duel::Request::Command { name: "soumatou", arguments: Some(Box::new(Netplayer::Observer(index))) }).ok();
        }
    }
}

#[after(ygopro::ClientJoin)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_client_join(duel: &mut Duel, join: &mut ygopro::ClientJoin, response: &mut Response) {
    if duel.stage > DuelStage::Begin {
        if let Some(oneshot) = join.position_sender.take() {
            *response = Response::Continue;
            let player = crate::player::BaseDuelPlayer::new(join.stoc_sender.clone());
            let undecided_index = duel.uninit_players.insert(player);
            duel.sender.undecided.insert(join.stoc_sender.clone());
            oneshot.send(Netplayer::Undecided(undecided_index as u8)).ok();
        } else {
            warn!("ClientJoin try to send the position, but find it already taken.")
        }
    }
}

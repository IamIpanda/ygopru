use std::ops::Deref;
use std::ops::DerefMut;

use parking_lot::ArcMutexGuard;

use parking_lot::RawMutex;
use ygopro_core_wrapper::Duel;
use ygopro_data::constants::DuelStage;
use ygopro_data::constants::Netplayer;
use ygopro_data::message::HostInfo;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_data::string::FixedLengthString;
use ygopro_handler::FromRequest;
use ygopro_handler::handler::Bundle;
use ygopro_handler::sync_handler::SyncHandler;

pub struct State<Duel> {
    pub guard: ArcMutexGuard<RawMutex, Duel>,
    pub player: Netplayer,
}

impl<Duel> Deref for State<Duel> {
    type Target = Duel;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<Duel> DerefMut for State<Duel> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<Duel, Res> FromRequest<ctos::Message, State<Duel>, Res> for Netplayer
where
    Duel: Send + Sync,
    Res: Send,
{
    fn from_request(bundle: &mut Bundle<ctos::Message, State<Duel>, Res>) -> Option<Self> {
        Some(bundle.state.player)
    }
}

pub type Response = ygopro_handler::extract::Response<stoc::Message>;
pub type Handler<Duel> = SyncHandler<ctos::Message, State<Duel>, Response>;

pub struct DuelPlayer {
    pub name: FixedLengthString<20>,
    /// The next CTOS message type this player is allowed to send.
    /// None means no restriction.
    pub state: Option<ctos::MessageType>,
}

pub struct DuelMode {
    pub host_player: Netplayer,
    pub host_info: HostInfo,
    pub duel_stage: DuelStage,
    pub duel: Duel,
    pub name: FixedLengthString<20>,
    pub pass: FixedLengthString<20>,
}

use std::ops::Deref;
use std::ops::DerefMut;

use tokio::sync::mpsc;

use ygopro_core_wrapper as core;
use ygopro_data::constants::DuelStage;
use ygopro_data::constants::Netplayer;
use ygopro_data::message::HostInfo;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_data::string::FixedLengthString;
use ygopro_handler::sync_handler::SyncHandler;

pub type Request = ygopro_handler::extract::Request<ctos::Message, Netplayer>; 
pub type Response = ygopro_handler::extract::Response<stoc::Message>;
pub type Handler<Duel> = SyncHandler<Request, State<Duel>, Response>;

pub struct State<Duel: 'static> {
    pub duel: Duel
}

impl<Duel> Deref for State<Duel> {
    type Target = Duel;

    fn deref(&self) -> &Self::Target {
        &self.duel
    }
}

impl<Duel> DerefMut for State<Duel> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.duel
    }
}

pub struct DuelPlayer {
    pub name: FixedLengthString<20>,
    pub stoc_sender: mpsc::UnboundedSender<stoc::Message>,
    /// The next CTOS message type this player is allowed to send.
    /// None means no restriction.
    pub state: Option<ctos::MessageType>,
}

impl DuelPlayer {
    pub fn new(stoc_sender: mpsc::UnboundedSender<stoc::Message>) -> Self {
        Self {
            name: FixedLengthString::new(String::new()),
            stoc_sender,
            state: None
        }
    }

    pub fn allow_message(&self, message: &ctos::Message) -> bool {
        let message_type = ctos::MessageType::from(message);
        match message_type {
            ctos::MessageType::Chat | ctos::MessageType::Surrender | ctos::MessageType::RequestField => true,
            _ if let Some(state) = self.state => state == message_type,
            _ => true
        }
    }
}

// fuck rust compiler
impl AsMut<DuelPlayer> for DuelPlayer {
    fn as_mut(&mut self) -> &mut DuelPlayer { self }
}

pub struct Duel {
    pub host_player: Netplayer,
    pub host_info: HostInfo,
    pub stage: DuelStage,
    pub duel: core::Duel,
    pub name: FixedLengthString<20>,
    pub pass: FixedLengthString<20>,
}

impl Deref for Duel {
    type Target = core::Duel;

    fn deref(&self) -> &Self::Target {
        &self.duel
    }
}

impl DerefMut for Duel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.duel
    }
}

#[derive(Clone, Copy)]
pub enum SendTarget {
    Single(Netplayer),
    Except(Netplayer),
    All,
    AllPlayer,
    AllObserver,
    None
}

impl From<Netplayer> for SendTarget {
    fn from(value: Netplayer) -> Self {
        SendTarget::Single(value)
    }
}


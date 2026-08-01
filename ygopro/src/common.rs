use std::io::Cursor;
use std::ops::Deref;
use std::ops::DerefMut;

use binrw::BinRead;
use tokio::sync::mpsc;

use ygopro_core_wrapper as core;
use ygopro_data::constants::CorePlayer;
use ygopro_data::constants::DuelStage;
use ygopro_data::constants::Location;
use ygopro_data::constants::Netplayer;
use ygopro_data::constants::Query;
use ygopro_data::data::CardPosition;
use ygopro_data::data::UpdateCardInfo;
use ygopro_data::message::HostInfo;
use ygopro_data::message::ctos;
use ygopro_data::message::gm;
use ygopro_data::message::gm::CardCode;
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

pub struct DuelPlayer<Message> {
    pub name: FixedLengthString<20>,
    pub stoc_sender: mpsc::UnboundedSender<Message>,
    /// The next CTOS message type this player is allowed to send.
    /// None means no restriction.
    pub state: Option<ctos::MessageType>,
}

impl<Message> DuelPlayer<Message> {
    pub fn new(stoc_sender: mpsc::UnboundedSender<Message>) -> Self {
        Self {
            name: FixedLengthString::new(String::new()),
            stoc_sender,
            state: None
        }
    }

    pub fn allow_message(&self, message: &ctos::Message) -> bool {
        let message_type = ctos::MessageType::from(message);
        match message_type {
            ctos::MessageType::Chat | ctos::MessageType::Surrender | ctos::MessageType::LeaveGame | ctos::MessageType::RequestField => true,
            _ if let Some(state) = self.state => state == message_type,
            _ => true
        }
    }
}

// fuck rust compiler
impl<Message> AsMut<DuelPlayer<Message>> for DuelPlayer<Message> {
    fn as_mut(&mut self) -> &mut DuelPlayer<Message> { self }
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

pub fn default_query(location: Location) -> Query {
    let every_one_want_this_query: Query = Query::Code | Query::Position | Query::Alias | Query::Type
                                            | Query::Level | Query::Rank | Query::Attribute | Query::Race
                                            | Query::Attack | Query::Defense | Query::BaseAttack | Query::BaseDefense
                                            | Query::Reason;
    match location {
        Location::Extra => Query::Link | Query::LeftScale | Query::RightScale | Query::Status | every_one_want_this_query,
        Location::Grave | Location::Removed => Query::Status | every_one_want_this_query,
        Location::Hand | Location::SZone => Query::LeftScale | Query::RightScale | Query::Status | every_one_want_this_query,
        Location::MZone => Query::Link | Query::Status | every_one_want_this_query,
        _ => Query::all()
    }
    
    
}

impl Duel {
    pub fn query_location_cards(&self, buf: &mut[u8], player: CorePlayer, location: Location, query: Query) -> gm::Message {
        let data_size = self.query_field_card(player, location, query, buf, false) as usize;
        let mut cursor = Cursor::new(&buf[..data_size]);
        let cards: Vec<UpdateCardInfo> = (0..).map_while(|_| UpdateCardInfo::read_le(&mut cursor).ok()).collect();
        gm::UpdateData { player, location, data: cards }.into()
    }

    pub fn refresh_location(&self, buf: &mut [u8], player: CorePlayer, locations: Location, query: Query) -> Vec<gm::Message> {
        let mut messasges = vec![];
        let players: &[CorePlayer] = if player == CorePlayer::All {
            &[CorePlayer::FirstAttackPlayer, CorePlayer::SecondAttackPlayer]
        } else {
            std::slice::from_ref(&player)
        };
        for &player in players {
            for location in Location::iter(&locations) {
                let query = if query.is_empty() { default_query(location) } else { query };
                messasges.push(self.query_location_cards(buf, player, location, query));
            }
        }
        messasges
    }

    pub fn refresh_card(&self, buf: &mut [u8], player: CorePlayer, location: Location, sequence: i8, mut query: Query) -> gm::Message {
        if query.is_empty() { query = Query::from_bits_retain(0xf81fff); }
        let len = self.query_card(player, location, sequence as u8, query, buf, false) as usize;
        let mut cursor = Cursor::new(&buf[..len]);
        let card = UpdateCardInfo::read_le(&mut cursor).ok().unwrap_or(UpdateCardInfo::Empty);
        gm::UpdateCard { 
            position: CardPosition::<false, false, false> { 
                code: CardCode::new(),
                controller: player,
                location,
                sequence: sequence as i8,
                sub_sequence: 0,
                description: 0,
            },
            data: card,
        }.into()
    }
}

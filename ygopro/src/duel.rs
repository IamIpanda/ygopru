//! Basic duel functionalities.
use std::any::Any;
use std::io::Cursor;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ops::DerefMut;

use binrw::BinRead;
use bytes::BytesMut;
use slab::Slab;
use tokio::sync::mpsc;

use ygopro_core_wrapper as core;
use ygopro_data::complex::Complex;
use ygopro_data::constants::*;
use ygopro_data::data::CardPosition;
use ygopro_data::data::UpdateCardInfo;
use ygopro_data::message::HostInfo;
use ygopro_data::message::gm::GameMessage;
use ygopro_data::message::gm::MaskedClone;
use ygopro_data::message::{ctos, stoc, gm};
use ygopro_data::message::gm::CardCode;
use ygopro_data::string::FixedLengthString;
use ygopro_handler::FromRequest;

use crate::Configuration;
use crate::ygopro_handlers;

/// Log the handler counts of each enabled plugin at debug level.
pub(crate) fn log_plugin_statistics(
    enabled_plugins: &hashbrown::HashSet<String>,
    ygopro_handler_counts: &hashbrown::HashMap<&'static str, usize>,
    ygopro_ex_handler_counts: &hashbrown::HashMap<&'static str, usize>,
    ygocore_handler_counts: &hashbrown::HashMap<&'static str, usize>,
    command_counts: &hashbrown::HashMap<&'static str, usize>,
) {
    let mut sorted_plugins = enabled_plugins.iter().collect::<Vec<_>>();
    sorted_plugins.sort();
    log::debug!("enabled plugins and their handlers:");
    for plugin_name in sorted_plugins {
        let ygopro_count = ygopro_handler_counts.get(plugin_name.as_str()).copied().unwrap_or(0);
        let ygopro_ex_count = ygopro_ex_handler_counts.get(plugin_name.as_str()).copied().unwrap_or(0);
        let ygocore_count = ygocore_handler_counts.get(plugin_name.as_str()).copied().unwrap_or(0);
        let command_count = command_counts.get(plugin_name.as_str()).copied().unwrap_or(0);
        if ygopro_count == 0 && ygopro_ex_count == 0 && ygocore_count == 0 && command_count == 0 { continue; }
        let handler_parts = [(ygopro_count, "ygopro handlers"), (ygopro_ex_count, "ygopro ex handlers"), (ygocore_count, "ygocore handlers"), (command_count, "command")]
            .into_iter()
            .filter(|(count, _)| *count > 0)
            .map(|(count, label)| format!("{count} {label}"))
            .collect::<Vec<_>>()
            .join(", ");
        let has_configuration = crate::plugin::CONFIGURATIONS.iter().any(|(name, _)| *name == plugin_name.as_str());
        let configuration_part = if has_configuration { ", configured" } else { "" };
        log::debug!("  {plugin_name}: {handler_parts}{configuration_part}");
    }
}

/// The target a [`stoc::Message`] would send to.
#[derive(Clone, Copy, Default)]
pub enum SendTarget {
    /// Send message to a single player.
    Single(Netplayer),
    /// Send message to all audience, but except the target player.
    Except(Netplayer),
    /// Send message to a player, targeted by ygocore.
    Core(CorePlayer),
    /// Send message to all audience.
    #[default]
    All,
    /// Send message to all players, but skip observers.
    AllPlayer,
    /// Send message to all observers, but skip players.
    AllObserver,
    /// Don't send this message.
    None
}

impl From<Netplayer> for SendTarget {
    fn from(value: Netplayer) -> Self {
        SendTarget::Single(value)
    }
}

impl From<CorePlayer> for SendTarget {
    fn from(value: CorePlayer) -> Self {
        SendTarget::Core(value)
    }
}

impl<Message, State, Res> FromRequest<ygopro_handler::extract::Request<Message, SendTarget>, State, Res> for &mut SendTarget
where State: Send, Res: Send, Message: Send {
    fn from_request(bundle: &mut ygopro_handler::Bundle<ygopro_handler::extract::Request<Message, SendTarget>, State, Res>) -> Option<Self> {
        Some(unsafe { &mut *(&mut bundle.request.extra as *mut SendTarget) })
    }
}

/// Abstract of a [`stoc::Message`] that will be sent.
/// During the sending, the message may change due to different target.
pub trait SendableMessage {
    /// Get an copy from this message towards target player.
    fn resolve(&self, player_index: Netplayer) -> Complex<stoc::Message>;
    /// Transform this message to the form towards target player.
    fn into_inner(self, player_index: Netplayer) -> Complex<stoc::Message>;
}

impl SendableMessage for Complex<stoc::Message> {
    fn resolve(&self, _: Netplayer) -> Complex<stoc::Message> {
        self.clone()
    }
    
    fn into_inner(self, _: Netplayer) -> Complex<stoc::Message> {
        self
    }
}

/// A [`gm::Message`] which may be masked.
/// 
/// See also: [`MaskedClone::clone_masked`] for how the masked copy is generated.
struct MaymaskedMessage<F> {
    pub message: Complex<stoc::Message>,
    pub masked_message: Complex<stoc::Message>,
    pub mask_judger: F
}

impl<F> SendableMessage for MaymaskedMessage<F> where F: Fn(Netplayer) -> bool {
    fn resolve(&self, player_index: Netplayer) -> Complex<stoc::Message> {
        if (self.mask_judger)(player_index) {
            self.message.clone()
        } else {
            self.masked_message.clone()
        }
    }
    
    fn into_inner(self, player: Netplayer) -> Complex<stoc::Message> {
        if (self.mask_judger)(player) {
            self.message
        } else {
            self.masked_message
        }
    }
}

impl<F> MaymaskedMessage<F> {
    /// Create an message by target [`gm::Message`], with an function juding that if that message need to be masked.
    pub fn new(message: gm::Message, f: F) -> Self {
        let masked_message = message.clone_masked();
        Self {
            message: Complex::from_message(stoc::Message::from(message)),
            masked_message: Complex::from_message(stoc::Message::from(masked_message)),
            mask_judger: f,
        }
    }
}

/// A struct that can transform a [`CorePlayer`] to [`SendTarget`].
pub trait CorePlayerToSendTarget {
    /// Transform a [`CorePlayer`] to a [`SendTarget`].
    fn transform(&self, player: CorePlayer) -> SendTarget;
}

/// The message dispatcher of a duel.
/// 
/// It routes [`stoc::Message`] to players and observers, and records every sent message.
pub struct Sender {
    /// Player senders, indexed by [`PlayerIndex`].
    pub players: Vec<mpsc::UnboundedSender<Complex<stoc::Message>>>,
    /// Observer senders.
    pub observers: Slab<mpsc::UnboundedSender<Complex<stoc::Message>>>,
    /// Senders of players who joined but have not yet sent [`ctos::PlayerInfo`] and [`ctos::JoinGame`].
    pub undecided: Slab<mpsc::UnboundedSender<Complex<stoc::Message>>>, 
    /// Every sent message, not used for now. 
    pub messages: Vec<Complex<stoc::Message>>,
    /// The masked copies of every sent message, used in joining game in middle.
    /// 
    /// See also: [`crate::plugin::soumatou`].
    pub masked_messages: Vec<Complex<stoc::Message>>
}

impl Sender {
    /// Create an empty sender.
    pub fn new() -> Self {
        Self {
            players: Vec::new(),
            observers: Slab::new(),
            undecided: Slab::new(),
            messages: Vec::new(),
            masked_messages: Vec::new(),
        }
    }

    /// Send a standard message to the target, recording it for replay.
    pub fn send(&mut self, message: stoc::Message, target: SendTarget) {
        let complex_message = Complex::from_message(message);
        self.messages.push(complex_message.clone());
        self.masked_messages.push(complex_message.clone());
        self.send_without_record(complex_message, target);
    }

    /// Send a game message, masking it per-player.
    /// 
    /// The masked copy is recorded only when the message is not a prompt waiting for a response.
    pub fn send_game_message(&mut self, message: gm::Message, target: SendTarget, mask_judger: impl Fn(Netplayer) -> bool, core_transformer: impl CorePlayerToSendTarget) {
        let mut target = target;
        match target {
            SendTarget::Core(player) => target = core_transformer.transform(player),
            _ => (),
        }
        let is_waiting_for = message.waiting_for();
        let maymasked = MaymaskedMessage::new(message, mask_judger);
        self.messages.push(maymasked.message.clone());
        if is_waiting_for.is_none() {
            self.masked_messages.push(maymasked.masked_message.clone());
        }
        self.send_without_record(maymasked, target);
    }

    /// Dispatch a message without recording it.
    pub(crate) fn send_without_record(&self, message: impl SendableMessage, target: SendTarget) {
        match target {
            SendTarget::Single(netplayer) => match netplayer {
                Netplayer::Player(index) => Sender::_send_single(message.into_inner(netplayer), self.players.get(index as usize)),
                Netplayer::Observer(index) => Sender::_send_single(message.into_inner(netplayer), self.observers.get(index as usize)),
                Netplayer::Undecided(index) => Sender::_send_single(message.into_inner(netplayer), self.undecided.get(index as usize)),
                Netplayer::Unknown => log::warn!("Try to send a message to unknown."),
            },
            SendTarget::Except(netplayer) => {
                let players = self.players.iter().enumerate()
                    .filter(|(index, _)| !matches!(netplayer, Netplayer::Player(i) if i as usize == *index))
                    .map(|(index, sender)| (Netplayer::Player(index as u8), sender));
                let observers = self.observers.iter()
                    .filter(|(index, _)| !matches!(netplayer, Netplayer::Observer(i) if i as usize == *index))
                    .map(|(index, sender)| (Netplayer::Observer(index as u8), sender));
                Sender::_send_iter(&message, players.chain(observers));
            }
            SendTarget::Core(_) => { log::warn!("Try to send message to a core player without transforming") },
            SendTarget::All => Sender::_send_iter(&message,
                self.players.iter().enumerate()
                    .map(|(index, sender)| (Netplayer::Player(index as u8), sender))
                    .chain(self.observers.iter()
                        .map(|(index, sender)| (Netplayer::Observer(index as u8), sender))),
            ),
            SendTarget::AllPlayer => Sender::_send_iter(&message,
                self.players.iter().enumerate()
                    .map(|(index, sender)| (Netplayer::Player(index as u8), sender)),
            ),
            SendTarget::AllObserver => Sender::_send_iter(&message,
                self.observers.iter()
                    .map(|(index, sender)| (Netplayer::Observer(index as u8), sender)),
            ),
            SendTarget::None => (),
        }
    }

    fn _send_iter<'a>(message: &impl SendableMessage, iter: impl Iterator<Item = (Netplayer, &'a mpsc::UnboundedSender<Complex<stoc::Message>>)>) {
        for (netplayer, target) in iter {
            Sender::_send(message.resolve(netplayer), target);
        }
    }

    fn _send_single(message: Complex<stoc::Message>, target: Option<&mpsc::UnboundedSender<Complex<stoc::Message>>>) {
        if let Some(target) = target {
            Sender::_send(message, target);
        }
    }

    fn _send(message: Complex<stoc::Message>, target: &mpsc::UnboundedSender<Complex<stoc::Message>>) {
        target.send(message).ok();
    }
}

impl Sender {
    /// Bind a player's sender at the given index, growing the slot list if the player joins out of order.
    pub(crate) fn set_player(&mut self, index: usize, sender: mpsc::UnboundedSender<Complex<stoc::Message>>) {
        if self.players.len() <= index { self.players.resize(index + 1, Self::dummy_sender()); }
        self.players[index] = sender;
    }

    /// Detach a player's sender at the given index, keeping the slot occupied.
    pub(crate) fn clear_player(&mut self, index: usize) {
        if self.players.len() <= index { self.players.resize(index + 1, Self::dummy_sender()); }
        self.players[index] = Self::dummy_sender();
    }

    /// Fill empty player slots with a sender whose receiver is dropped.
    fn dummy_sender() -> mpsc::UnboundedSender<Complex<stoc::Message>> {
        let (sender, _receiver) = mpsc::unbounded_channel();
        sender
    }
}

/// The request sent to [`Duel`] in actor model.
pub enum Request {
    /// A standard [`ctos::Message`].
    Message(ygopro_handlers::Request),
    /// A extended internal [`ygopro::Message`](crate::message::Message).
    MessageEx(ygopro_handlers::RequestEx),
    /// Make internal ygocore engine evolve its state.
    /// That should be a command, but too important.
    Evolve,
    /// A custom command.
    Command { name: &'static str, arguments: Option<Box<dyn Any + Send>> }
}

type BaseDuelPlayer = crate::player::BaseDuelPlayer<Complex<stoc::Message>>;
type DuelPlayer = crate::player::DuelPlayer<Complex<stoc::Message>>;

/// Strict [`Netplayer`].
#[derive(Copy, Clone, Eq, PartialEq, Debug, PartialOrd, Ord, Hash)]
pub struct PlayerIndex(pub u8);

#[allow(non_upper_case_globals)]
impl PlayerIndex {
    /// The player at slot 1.
    pub const Player1: PlayerIndex = PlayerIndex(0);
    /// The player at slot 2.
    pub const Player2: PlayerIndex = PlayerIndex(1);
    /// The player at slot 3.
    pub const Player3: PlayerIndex = PlayerIndex(2);
    /// The player at slot 4.
    pub const Player4: PlayerIndex = PlayerIndex(3);
}

impl From<PlayerIndex> for Netplayer {
    fn from(value: PlayerIndex) -> Self {
        Netplayer::Player(value.0)
    }
}

impl TryFrom<Netplayer> for PlayerIndex {
    type Error = ();

    fn try_from(value: Netplayer) -> Result<Self, Self::Error> {
        match value {
            Netplayer::Player(index) => Ok(PlayerIndex(index)),
            _ => Err(())
        }
    }
}

impl From<PlayerIndex> for SendTarget {
    fn from(value: PlayerIndex) -> Self {
        let player: Netplayer = value.into();
        player.into()
    }
}

impl From<u8> for PlayerIndex {
    fn from(value: u8) -> Self {
        PlayerIndex(value)
    }
}

/// Basic Duel container.
///
/// `Duel` works in the actor model. In theory it is a
/// [`RoomProvider`](ygopro_handler::RoomProvider): it takes a stream of
/// [`ctos::Message`] and returns a stream of
/// [`stoc::Message`]. In practice the input type is
/// [`ygopro_handlers::Request`], so custom control messages can be inserted alongside the
/// wire messages.
///
/// Every input `ctos::Message` is handled by a function in the `ygopro_handlers` module,
/// registered into [`ygopro_handlers::YGOPRO_HANDLERS`]. Registering a new function there
/// controls how a message is processed. In principle, the `stoc` message that directly
/// answers an input `ctos` message should be returned via
/// [`Response::Replace`](ygopro_handler::extract::Response::Replace) /
/// [`Response::ReplaceMultiple`](ygopro_handler::extract::Response::ReplaceMultiple),
/// while a message caused by another player's action is sent directly through [`Sender`].
/// In practice the returned message gets no further processing or hook, so there is no
/// difference.
///
/// `Duel` advances the internal ygocore state via [`Request::Evolve`]; sending a
/// [`ctos::Response`] triggers it automatically.
/// Once the core returns its messages, they are routed through
/// [`ygocore_handlers::YGOCORE_HANDLERS`](crate::ygocore_handlers::YGOCORE_HANDLERS), which also sends the refresh messages and sets
/// the message gate. Registering a function into that distributed slice controls this
/// behavior.
///
/// Duel does its best to keep the binary message protocol and its timing identical to ygopro. 
/// But for extensibility and implementation details, some messages differ from the original:
/// - A [`CreateGame`](ygopro_data::message::ctos::CreateGame) message is sent to the duel
///   when it is created. It does not exist in the original ygopro; it initializes the duel
///   data.
/// - The `ygocore` duel is created when the room is created
///   ([`DuelHost::new`](crate::host::DuelHost::new)), unlike the original which creates it
///   in [`TpResult`](ygopro_data::message::ctos::TpResult).
/// - When a network player leaves, a [`LeaveGame`](ygopro_data::message::ctos::LeaveGame)
///   message is sent to the duel. It does not exist in the original ygopro; it lets plugins
///   catch that moment.
/// 
/// At the start and the end of a duel, `Duel` triggers a chain of its own message type
/// [`ygopro::Message`](crate::message::Message), so plugins can modify these behaviors.
/// See [`message`](crate::message).
///
/// `Duel` does not handle the conversions between [`Netplayer`] and [`CorePlayer`], nor
/// between [`PlayerIndex`] and [`SendTarget`]; those belong to its
/// subclasses ([`SingleDuel`](crate::single_duel::SingleDuel),
/// [`TagDuel`](crate::tag_duel::TagDuel)). This means some behavior cannot be handled at
/// the `Duel` level, so certain plugins have to register two functions, one into
/// [`SINGLE_DUEL_YGOPRO_HANDLERS`](crate::single_duel::ygopro_handlers::SINGLE_DUEL_YGOPRO_HANDLERS)
/// and one into
/// [`TAG_DUEL_YGOPRO_HANDLERS`](crate::tag_duel::ygopro_handlers::TAG_DUEL_YGOPRO_HANDLERS).
/// These handler functions are mixed together in the same
/// [`Processor`](ygopro_handler::Processor).
///
pub struct Duel {
    /// Internal ygocore duel instance.
    pub core: core::Duel,
    /// Duel name. It is often empty.
    pub name: FixedLengthString<20>,
    /// Duel password. It is often empty.
    pub pass: FixedLengthString<20>,
    /// Host player position.
    /// 
    /// Please note that host player can be an observer.
    pub host_player: Netplayer,
    /// Duel info defined by origin ygopro protocol.
    pub host_info: HostInfo,
    /// Stage this duel is current in.
    pub stage: DuelStage,
    /// The message sender that routes messages to players and observers.
    pub sender: Sender,
    /// A buffer that used to serialize [`ctos::Response`].
    pub response_buffer: BytesMut,
    /// A buffer that used to deserialzie [`gm::Message`] from ygocore.
    pub core_request_buffer: BytesMut,
    /// Player slot in current duel.
    /// 
    /// This vector should be in a fixed length.
    pub players: Vec<Option<DuelPlayer>>,
    /// Max player count, should always be `players.len()`.
    pub max_player_count: usize,
    /// Observers in current duel.
    pub observers: Slab<BaseDuelPlayer>,
    /// A saver that mark if this duel is ended by a match kill effect.
    pub match_kill_card_code: i32,
    /// How many duels this game has finished.
    pub duel_count: u8,
    /// Who decide the first attack player in current duel.
    pub first_attack_decider: Option<PlayerIndex>,
    /// The last sent select message. 
    pub last_select_message: Option<gm::Message>,
    /// The last player sent the [`ctos::Response`].
    pub last_response: Option<PlayerIndex>,
    // extended by rust ygopro
    /// Extra duel info defined by ygopru.
    /// 
    /// This field is extended by ygopru.
    pub configuration: Configuration,
    /// A buffer that saves player who have already joined in , but haven't sent the [`ctos::PlayerInfo`] and [`ctos::JoinGame`].
    pub uninit_players: Slab<BaseDuelPlayer>,
    // replay recorder
    /// The duel start time.
    /// 
    /// Only used by generating replay.
    pub start_time: u32,
    /// All [`ctos::Response`] message sent by players.
    /// 
    /// Only used by generating replay.
    pub client_responses: Vec<ctos::Response>,
    // extended by actor models
    /// A mpsc sender, which make outbound can send [`Request`] to duel.
    pub request_sender: mpsc::UnboundedSender<Request>,
    pub(crate) request_receiver: Option<mpsc::UnboundedReceiver<Request>>,
}

impl Duel {
    /// Create a duel, defined by a [`HostInfo`] and [`struct@Configuration`].
    pub fn new(host_info: HostInfo, mut configuration: Configuration) -> Self {
        let seed = configuration.seed(0);
        let (request_sender, request_receiver) = mpsc::unbounded_channel();
        Self {
            host_player: Netplayer::Unknown,
            host_info,
            stage: DuelStage::Begin,
            core: core::Duel::new(seed),
            name: FixedLengthString::allocate(),
            pass: FixedLengthString::allocate(),
            sender: Sender::new(),
            response_buffer: BytesMut::zeroed(core::SIZE_RETURN_VALUE),
            core_request_buffer: BytesMut::zeroed(core::SIZE_QUERY_BUFFER),
            players: Vec::new(),
            max_player_count: 0,
            observers: Slab::new(),
            match_kill_card_code: -1,
            duel_count: 0,
            first_attack_decider: None,
            last_select_message: None,
            last_response: None,
            configuration,
            uninit_players: Slab::new(),
            start_time: 0,
            client_responses: Vec::new(),
            request_sender,
            request_receiver: Some(request_receiver),
        }
    }

    /// Get a player by [`PlayerIndex`].
    pub fn get(&self, index: PlayerIndex) -> Option<&DuelPlayer> {
        self.players.get(index.0 as usize)?.as_ref()
    }

    /// Get a player by [`Netplayer`].
    pub fn get_net(&self, index: Netplayer) -> Option<&DuelPlayer> {
        let Netplayer::Player(index) = index else { return None };
        self.players.get(index as usize)?.as_ref()
    }

    /// Get a mutable player by [`PlayerIndex`].
    pub fn get_mut(&mut self, index: PlayerIndex) -> Option<&mut DuelPlayer> {
        self.players.get_mut(index.0 as usize)?.as_mut()
    }

    /// Get a mutable player by [`Netplayer`].
    pub fn get_net_mut(&mut self, index: Netplayer) -> Option<&mut DuelPlayer> {
        let Netplayer::Player(index) = index else { return None };
        self.players.get_mut(index as usize)?.as_mut()
    }

    /// Get multiple player by [`PlayerIndex`].
    pub fn get_many_mut<const L: usize>(&mut self, index: [PlayerIndex; L]) -> [&mut Option<DuelPlayer>; L] {
        let player_count = self.players.len();
        let players_ptr = self.players.as_mut_ptr();
        let mut result: [MaybeUninit<&mut Option<DuelPlayer>>; L] = std::array::from_fn(|_| MaybeUninit::uninit());
        for (slot, player_index) in index.iter().copied().enumerate() {
            let position = player_index.0 as usize;
            assert!(position < player_count, "player index out of bounds");
            assert!(!index[..slot].contains(&player_index), "duplicate player index");
            unsafe {
                result[slot].write(&mut *players_ptr.add(position));
            }
        }
        unsafe { result.map(|slot| slot.assume_init()) }
    }

    /// Sends a standard `ctos` message to the [`Duel`] actor, targeting a single [`Netplayer`].
    ///
    /// The message is wrapped in a [`Request::Message`] along with the receiver, so the duel can
    /// route the response to the correct player.
    ///
    /// See Also
    /// --------
    /// * [`ctos::Message`] — the standard client-to-server message enum queued by this method.
    pub fn queue_request<Message: Into<ctos::Message>>(&self, message: Message, player: Netplayer) {
        self.request_sender.send(Request::Message( ygopro_handlers::Request { message: message.into(), extra: player } )).ok();
    }

    /// Sends an extended internal message to the [`Duel`] actor, targeting all receivers.
    ///
    /// Unlike [`queue_request`](Self::queue_request), the message type is the internal
    /// [`ygopro::Message`](crate::message::Message) rather than a wire-level `ctos` message, and its
    /// [`SendTarget`] defaults to all.
    ///
    /// See Also
    /// --------
    /// * [`message`](crate::message) — the module defining the internal `MessageEx` messages.
    /// * [`ygopro::Message`](crate::message::Message) — the internal message enum queued by this method.
    pub fn queue_request_ex<Message: Into<crate::message::Message>>(&self, message: Message) {
        self.request_sender.send(Request::MessageEx( ygopro_handlers::RequestEx { message: message.into(), extra: SendTarget::All } )).ok();
    }

    /// Queues a custom command for the [`Duel`] actor, optionally carrying arbitrary arguments.
    ///
    /// The command is dispatched by name through the [`command::COMMANDS`](crate::command::COMMANDS) registry and its
    /// argument box is forwarded as-is.
    ///
    /// See Also
    /// --------
    /// * [`command`](crate::command) — the module defining commands and their handlers.
    /// * [`command::COMMANDS`](crate::command::COMMANDS) — the command name to handler registry.
    /// * [`command::CommandHandler`](crate::command::CommandHandler) — the handler type invoked for a command.
    pub fn queue_command(&self, command: &'static str, args: Option<Box<dyn Any + Send>>) {
        self.request_sender.send(Request::Command { name: command, arguments: args }).ok();
    }

}

impl Deref for Duel {
    type Target = core::Duel;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl DerefMut for Duel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.core
    }
}

impl Duel {
    /// Get a query, which is often used by target [`Location`].
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

    /// Query target location, return the cards at that location.
    /// 
    /// That query the ygocore.
    pub fn query_location_cards(&mut self, player: CorePlayer, location: Location, query: Query) -> gm::Message {
        let data_size = self.core.query_field_card(player, location, query, &mut self.core_request_buffer, false) as usize;
        let mut cursor = Cursor::new(&self.core_request_buffer[..data_size]);
        let cards: Vec<UpdateCardInfo> = (0..).map_while(|_| UpdateCardInfo::read_le(&mut cursor).ok()).collect();
        gm::UpdateData { player, location, data: cards }.into()
    }

    /// Query target location, and send response info to target player.
    /// 
    /// Leave Query empty to use [`default_query`](Duel::default_query).
    /// That query the ygocore.
    pub fn refresh_location(&mut self, player: CorePlayer, locations: Location, query: Query) -> Vec<gm::Message> {
        let mut messasges = vec![];
        let players: &[CorePlayer] = if player == CorePlayer::All {
            &[CorePlayer::FirstAttackPlayer, CorePlayer::SecondAttackPlayer]
        } else {
            std::slice::from_ref(&player)
        };
        for &player in players {
            for location in Location::iter(&locations) {
                let query = if query.is_empty() { Self::default_query(location) } else { query };
                messasges.push(self.query_location_cards(player, location, query));
            }
        }
        messasges
    }

    /// Query target card (defined by Location and sequence), and send response info to target player.
    /// 
    /// Leave Query empty to use the [`default_query`](Duel::default_query).
    /// That query the ygocore.
    pub fn refresh_card(&mut self, player: CorePlayer, location: Location, sequence: i8, mut query: Query) -> gm::Message {
        if query.is_empty() { query = Query::from_bits_retain(0xf81fff); }
        let len = self.core.query_card(player, location, sequence as u8, query, &mut self.core_request_buffer, false) as usize;
        let mut cursor = Cursor::new(&self.core_request_buffer[..len]);
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

    /// Send a game message to the target, masking it per-player when needed.
    pub fn send_game_message(&mut self, message: gm::Message, target: SendTarget, core_transformer: impl CorePlayerToSendTarget + PlayerConverter) {
        if message.waiting_for().is_some() { self.last_select_message = Some(message.clone()); }
        if self.configuration.no_mask {
            let target = match target {
                SendTarget::Core(player) => core_transformer.transform(player),
                _ => target,
            };
            self.sender.send(stoc::Message::from(message), target);
        } else {
            let can_player_see_unmasked: Vec<bool> = (0u8..(self.max_player_count as u8)).map(|index| !message.should_mask(core_transformer.to_core_player(Netplayer::Player(index)))).collect();
            let mask_judger = move |netplayer: Netplayer| matches!(netplayer, Netplayer::Player(index) if can_player_see_unmasked[index as usize]);
            self.sender.send_game_message(message, target, mask_judger, core_transformer);
        }
    }

    /// Refresh a card or location and send the update to all players.
    pub fn refresh(&mut self, player: CorePlayer, locations: Location, sequence: i8, query: Query, core_transformer: impl CorePlayerToSendTarget + PlayerConverter + Clone) {
        if sequence >= 0 {
            let message = self.refresh_card(player, locations, sequence, query);
            self.send_game_message(message, SendTarget::All, core_transformer.clone());
        } else {
            for message in self.refresh_location(player, locations, query).into_iter() {
                self.send_game_message(message, SendTarget::All, core_transformer.clone());
            }
        }
    }
}

/// Check whether a player's response is meaningful for the last select message.
pub fn response_is_meaningful(response: &ygopro_data::data::Response, last_select_message: &gm::Message) -> bool {
    if gm::MessageType::from(last_select_message) == gm::MessageType::Retry { return false; }
    let resolved = match response {
        ygopro_data::data::Response::Unknown(data) => {
            let mut resolved = ygopro_data::data::Response::Unknown(data.clone());
            resolved.resolve(gm::MessageType::from(last_select_message)).ok();
            Some(resolved)
        }
        _ => None,
    };
    let response = resolved.as_ref().unwrap_or(response);
    match response {
        ygopro_data::data::Response::Cancel => gm::MessageType::from(last_select_message) != gm::MessageType::SelectUnselectCard,
        ygopro_data::data::Response::DeclineChain | ygopro_data::data::Response::SelectUnselectCards(_) => false,
        ygopro_data::data::Response::SelectIdleCommand(command, _) => (*command) as u16 <= ygopro_data::data::IdleCommand::Activate as u16,
        ygopro_data::data::Response::SelectBattleCommand(command, _) => (*command) as u16 <= ygopro_data::data::BattleCommand::Attack as u16,
        _ => true,
    }
}

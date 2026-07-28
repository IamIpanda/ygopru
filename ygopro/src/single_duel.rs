use std::io::Cursor;
use std::ops::Deref;
use std::ops::DerefMut;

use log::warn;
use binrw::BinRead;
use futures::Stream;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use ygopro_core_wrapper::DuelSeed;
use ygopro_core_wrapper as core;
use ygopro_data::constants::*;
use ygopro_data::data::Deck;
use ygopro_data::data::Replay;
use ygopro_data::data::UpdateCardInfo;
use ygopro_data::data::CardPosition;
use ygopro_data::message::game_message::CardCode;
use ygopro_data::string::FixedLengthString;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_data::message::gm;
use ygopro_handler::RoomProvider;
use ygopro_handler::Bundle;
use ygopro_handler::FromRequest;
use ygopro_handler::MessageKey;

use crate::common;
use crate::common::Response;
use crate::common::SendTarget;
use crate::common::State;

pub fn init() {
    ygopro_handlers::reset_processor();
    ygocore_handlers::reset_processor(); 
}

pub enum Request {
    Join { stoc_sender: mpsc::UnboundedSender<stoc::Message> },
    Message(common::Request),
    TimerTick,
    Evolve
}

pub struct DuelPlayer {
    player: common::DuelPlayer,
    ready: bool,
    deck: Deck,
    hand: Option<Hand>,
    time_limit: u16,
    time_compensator: u16,
    time_backed: u16,
}

impl From<common::DuelPlayer> for DuelPlayer {
    fn from(value: common::DuelPlayer) -> Self {
        Self {
            player: value, 
            ready: false,
            deck: Deck::new(),
            hand: None,
            time_limit: 0,
            time_compensator: 0,
            time_backed: 0,
        }
    }
}

impl Deref for DuelPlayer {
    type Target = common::DuelPlayer;
    fn deref(&self) -> &Self::Target { &self.player }
}

impl DerefMut for DuelPlayer {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.player }
}

impl AsRef<common::DuelPlayer> for DuelPlayer {
    fn as_ref(&self) -> &common::DuelPlayer { &self.player }
}

impl AsMut<common::DuelPlayer> for DuelPlayer {
    fn as_mut(&mut self) -> &mut common::DuelPlayer { &mut self.player }
}

impl<Response> FromRequest<common::Request, State<SingleDuel>, Response> for &mut DuelPlayer where Request: Send + Sync, Response: Send {
    fn from_request(bundle: &mut Bundle<common::Request, State<SingleDuel>, Response>) -> Option<Self> {
        let player = bundle.state.duel.get_player_mut(bundle.request.extra)?;
        Some(unsafe { &mut *(player as *mut DuelPlayer) })
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PlayerIndex {
    Player1 = 0,
    Player2 = 1
}

impl PlayerIndex {
    pub fn opponent(self) -> Self {
        match self {
            PlayerIndex::Player1 => PlayerIndex::Player2,
            PlayerIndex::Player2 => PlayerIndex::Player1,
        }
    }
}

impl TryFrom<Netplayer> for PlayerIndex {
    type Error = ();

    fn try_from(value: Netplayer) -> Result<Self, Self::Error> {
        match value {
            Netplayer::Player(0) => Ok(PlayerIndex::Player1),
            Netplayer::Player(1) => Ok(PlayerIndex::Player2),
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

impl TryFrom<usize> for PlayerIndex {    
    type Error = ();
    
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Player1),
            1 => Ok(Self::Player2),
            _ => Err(())
        }
    }
}

impl Into<Netplayer> for PlayerIndex {
    fn into(self) -> Netplayer {
        Netplayer::Player(self as u8)
    }
}

impl<State, Res> FromRequest<common::Request, State, Res> for PlayerIndex where State: Send + Sync, Res: Send {
    fn from_request(bundle: &mut Bundle<common::Request, State, Res>) -> Option<Self> {
        Self::try_from(bundle.request.extra).ok()
    }
}

fn default_seed_generator(_match_count: u8) -> DuelSeed {
    return DuelSeed::None
}

pub struct SingleDuel {
    duel: common::Duel,
    players: [Option<DuelPlayer>; 2], 
    first_attack_player: Option<PlayerIndex>,
    first_attack_decider: Option<PlayerIndex>,
    last_response: Option<PlayerIndex>,
    is_match: bool,
    match_kill_card_code: i32,
    duel_count: u8,
    duel_winner: Vec<Option<PlayerIndex>>,
    time_elapsed: u16,
    // these fields are only for request_field.
    // that message are actually inner core.
    // that make ygopro works like srvpro, which make us think that should be a Room attachment instead.
    phase: Phase,
    deck_reversed: bool,
    // extended by rust ygopro
    seed_generator: fn(match_count: u8) -> DuelSeed,
    // extended by actor models
    messages: Vec<stoc::Message>,
    observers: Vec<Option<common::DuelPlayer>>,
    observer_messages: Vec<stoc::Message>,
    observer_sender: broadcast::Sender<stoc::Message>,
    observer_receiver: broadcast::Receiver<stoc::Message>,
    request_sender: mpsc::UnboundedSender<Request>,
    request_receiver: Option<mpsc::UnboundedReceiver<Request>>,
    last_init_player: Option<common::DuelPlayer>,
    timer_task: Option<tokio::task::JoinHandle<()>>,
}

impl SingleDuel {
    pub fn new(is_match: bool, seed_generator: Option<fn(u8) -> DuelSeed>) -> Self {
        let seed_generator = seed_generator.unwrap_or(default_seed_generator);
        let (observer_sender, observer_receiver) = broadcast::channel(128);
        let (request_sender, request_receiver) = mpsc::unbounded_channel();
        Self {
            duel: common::Duel {
                host_player: Netplayer::Unknown,
                host_info: Default::default(),
                stage: DuelStage::Begin,
                duel: core::Duel::new(seed_generator(0)),
                name: FixedLengthString::allocate(),
                pass: FixedLengthString::allocate(),
            },
            players: [None, None],
            first_attack_player: None,
            last_response: None,
            first_attack_decider: None,
            phase: Phase::Draw,
            deck_reversed: false,
            is_match,
            match_kill_card_code: 0,
            duel_count: 0,
            duel_winner: Vec::new(),
            time_elapsed: 0,
            seed_generator,
            observers: Vec::new(),
            messages: Vec::new(),
            observer_messages: Vec::new(),
            observer_sender,
            observer_receiver,
            request_sender,
            request_receiver: Some(request_receiver),
            last_init_player: None,
            timer_task: None,
        }
    }

    pub fn run(mut self) -> Option<tokio::task::JoinHandle<()>> {
        let receiver = self.request_receiver.take()?;
        let mut stream = UnboundedReceiverStream::new(receiver);

        let handle = tokio::spawn(async move {
            let ygopro_processor = ygopro_handlers::YGOPRO_PROCESSOR.get().expect("Processor not initialized").load_full();
            let ygocore_processor = ygocore_handlers::YGOCORE_PROCESSOR.get().expect("Processor not initialized").load_full();
            let mut duel = self;

            while let Some(request) = stream.next().await {
                match request {
                    Request::Join { stoc_sender } => {
                        if duel.last_init_player.is_some() { warn!("Two players are trying to init in the same duel.") }
                        duel.last_init_player = Some(common::DuelPlayer::new(stoc_sender));
                    },
                    Request::TimerTick => {
                        if let Some(last_response) = duel.last_response && duel.host_info.time_limit > 0 {
                            duel.time_elapsed = duel.time_elapsed.saturating_add(1);
                            let timed_out = duel.get_player_index(last_response)
                                    .map_or(false, |player| duel.time_elapsed >= player.time_limit);
                            if timed_out {
                                let loser = duel.to_core_player(last_response);
                                duel.win_and_end(loser, WinReason::Timeout);
                            }
                        }
                    },
                    Request::Message(request) => {
                        if ! duel.get_player(request.extra).map(|p| p.allow_message(&request.message)).unwrap_or(true) {
                            warn!("Message type mismatch for player: {:?}, get {:?}", request.extra, ctos::MessageType::from(&request.message));
                            continue;
                        }
                        let state = common::State { duel };
                        let bundle = Bundle { request, state, response: Default::default() };
                        let key = bundle.request.message_key();
                        let Bundle {
                            request,
                            state: common::State { duel: returned_duel },
                            response
                        } = ygopro_processor.process_bundle(bundle, key).await;
                        duel = returned_duel;
                        let position = request.extra;
                        match response {
                            ygopro_handler::extract::Response::Replace(message) => duel.send(message, position.into()),
                            ygopro_handler::extract::Response::ReplaceMultiple(messages) => {
                                let position = position.into();
                                for message in messages {
                                    duel.send(message, position);
                                };
                            },
                            ygopro_handler::extract::Response::Continue => {},
                            ygopro_handler::extract::Response::Swallow => {},
                            ygopro_handler::extract::Response::Stop => { break; }
                            ygopro_handler::extract::Response::Kick => duel.send(stoc::LeaveGame { pos: position }.into(), position.into())
                        };
                    }
                    Request::Evolve => {
                        let messages = ygocore_handlers::evolve(&mut duel);
                        let mut last_target = SendTarget::All;
                        for message in messages {
                            let key = message.message_key();
                            let request = ygocore_handlers::Request { message, extra: Netplayer::Unknown };
                            let state = common::State { duel };
                            let bundle = Bundle { request, state, response: Default::default() };
                            let Bundle {
                                request,
                                state: common::State { duel: returned_duel },
                                response
                            } = ygocore_processor.process_bundle(bundle, key).await;
                            returned_duel.send_game_message(request.message, response.target);
                            let (player, location, sequence) = response.refresh;
                            if sequence < 0 { returned_duel.refresh_zone(player, location); } 
                            else { returned_duel.refresh_single(player, location, sequence); }
                            last_target = response.target;
                            duel = returned_duel;
                        }
                        if let SendTarget::Single(Netplayer::Player(n)) = last_target {
                            let index = if n == 0 { PlayerIndex::Player1 } else { PlayerIndex::Player2 };
                            let core_player = duel.to_core_player(index);
                            ygocore_handlers::set_waiting(&mut duel, core_player);
                        }
                    }
                }
            }
        });

        Some(handle)
    }

    pub fn subscribe_observer(&self) -> impl Stream<Item = stoc::Message> {
        let history = tokio_stream::iter(self.observer_messages.clone());
        let live = BroadcastStream::new(self.observer_sender.subscribe())
            .filter_map(|result| result.ok());
        history.chain(live)
    }

    pub fn get_player(&self, player: Netplayer) -> Option<&DuelPlayer> {
        match player {
            Netplayer::Player(netplayer) => {
                self.players[netplayer as usize].as_ref()
            }
            _ => None,
        }
    }

    pub fn get_player_mut(&mut self, player: Netplayer) -> Option<&mut DuelPlayer> {
        match player {
            Netplayer::Player(netplayer) => {
                self.players[netplayer as usize].as_mut()
            }
            _ => None,
        }
    }

    pub fn get_player_index(&self, player: PlayerIndex) -> Option<&DuelPlayer> {
        self.players[player as u8 as usize].as_ref()
    }

    pub fn get_player_mut_index(&mut self, player: PlayerIndex) -> Option<&mut DuelPlayer> {
        self.players[player as u8 as usize].as_mut()        
    }

    pub fn observer_count(&self) -> u16 {
        self.observers.iter().fold(0, |s, v| { s + if v.is_some(){ 1 } else { 0 } }) as u16
    }

    pub fn insert_observer(&mut self, player: common::DuelPlayer) -> Netplayer {
        let slot = self.observers.iter().position(|v| v.is_none());
        let position = match slot {
            Some(slot) => {
                self.observers[slot] = Some(player);
                slot as u8
            }
            None => {
                self.observers.push(Some(player));
                self.observers.len() as u8 - 1
            }
        };
        Netplayer::Observer(position)
    }

    pub fn to_core_player(&self, player: PlayerIndex) -> CorePlayer {
        let first_attack_player = self.first_attack_player.unwrap_or(PlayerIndex::Player1);
        let core_player: CorePlayer = match player {
            PlayerIndex::Player1 => CorePlayer::FirstAttackPlayer,
            PlayerIndex::Player2 => CorePlayer::SecondAttackPlayer,
        };
        if first_attack_player == PlayerIndex::Player1 {
            core_player
        } else {
            core_player.opponent()
        }
    }

    pub fn to_player_index(&self, core_player: CorePlayer) -> Option<PlayerIndex> {
        let first_attack_player = self.first_attack_player.unwrap_or(PlayerIndex::Player1);
        let core_player = match first_attack_player {
            PlayerIndex::Player1 => core_player,
            _ => core_player.opponent()
        };
        let index = match core_player {
            CorePlayer::FirstAttackPlayer => PlayerIndex::Player1,
            CorePlayer::SecondAttackPlayer => PlayerIndex::Player2,
            _ => return None
        };
        Some(index)
    }

    pub fn to_net_player(&self, core_player: CorePlayer) -> Netplayer {
        let first_attack_player = self.first_attack_player.unwrap_or(PlayerIndex::Player1);
        let core_player = match first_attack_player {
            PlayerIndex::Player1 => core_player,
            _ => core_player.opponent()
        };
        match core_player {
            CorePlayer::FirstAttackPlayer => Netplayer::Player(0),
            CorePlayer::SecondAttackPlayer => Netplayer::Player(1),
            CorePlayer::None => Netplayer::Unknown,
            CorePlayer::All => Netplayer::Unknown,
            CorePlayer::Rule => Netplayer::Unknown,
        } 
    }

    pub fn calculate_replay(&self) -> Replay {
        todo!()
    }

    pub fn win_and_end(&mut self, loser: CorePlayer, reason: WinReason) {
        let winner = loser.opponent();
        let win_message = gm::Message::Win(gm::Win { winner, reason });
        self.send(stoc::GameMessage { message: win_message }.into(), SendTarget::All);
        let winner_netplayer = self.to_player_index(winner);
        self.duel_winner.push(winner_netplayer);
        self.first_attack_decider = Some(self.to_player_index(loser).unwrap_or(PlayerIndex::Player1));
        self.duel_end();
    }

    fn should_match_end(&self) -> bool {
        let end_count = if self.is_match { 3 } else { 1 };
        let end_win_count = (end_count + 1) / 2;
        let mut player_wins = [0, 0];
        for winner in &self.duel_winner {
            match winner {
                Some(PlayerIndex::Player1) => player_wins[0] += 1,
                Some(PlayerIndex::Player2) => player_wins[1] += 1,
                None => (),
            }
        }
        self.duel_winner.len() >= end_count || player_wins[0] >= end_win_count || player_wins[1] >= end_win_count || self.match_kill_card_code > 0
    }

    pub fn duel_end(&mut self) {
        if let Some(timer_task) = self.timer_task.take() {
            timer_task.abort();
        }
        let replay = self.calculate_replay();
        self.send(stoc::Replay{ replay: Box::new(replay) }.into(), SendTarget::All);
        self.duel.end();
        self.duel_count += 1;
        if self.should_match_end() {
            self.stage = DuelStage::End;
            self.send(stoc::DuelEnd.into(), SendTarget::All);
        } else {
            for i in [0,1] {
                if let Some(player) = self.players[i].as_mut() { 
                    player.state = Some(ctos::MessageType::UpdateDeck);
                    player.ready = false;
                }
            }
            self.first_attack_player = None;
            self.stage = DuelStage::Siding;
            self.send(stoc::ChangeSide.into(), SendTarget::AllPlayer);
            self.send(stoc::WaitingSide.into(), SendTarget::AllObserver);
            self.end();
            self.duel.duel = ygopro_core_wrapper::Duel::new((self.seed_generator)(self.duel_count));
        }
    }


    fn send_netplayer(&self, message: stoc::Message, target: Netplayer) {
        match target {
            Netplayer::Player(index) => {
                if let Some(player) = &self.players[index as usize] {
                    player.stoc_sender.send(message).ok();
                }
            }
            Netplayer::Observer(index) => {
                if let Some(Some(observer)) = &self.observers.get(index as usize) {
                    observer.stoc_sender.send(message).ok();
                }
            }
            Netplayer::Unknown => warn!("Try to send message to an unknown position.")
        }
    }

    fn send(&self, message: stoc::Message, target: SendTarget) {
        match target {
            SendTarget::Single(netplayer) => self.send_netplayer(message, netplayer),
            SendTarget::Except(netplayer) => {
                // todo: fix other situations.
                match netplayer {
                    Netplayer::Player(0) => self.send_netplayer(message.clone(), Netplayer::Player(1).into()),
                    Netplayer::Player(1) => self.send_netplayer(message.clone(), Netplayer::Player(0).into()),
                    _ => self.send(message.clone(), SendTarget::AllPlayer),
                }
                self.send(message, SendTarget::AllObserver);
            }
            SendTarget::All => {
                self.send(message.clone(), SendTarget::AllPlayer);
                self.send(message,         SendTarget::AllObserver);
            }
            SendTarget::AllPlayer => {
                self.send(message.clone(), Netplayer::Player(0).into());
                self.send(message,         Netplayer::Player(1).into());
            }
            SendTarget::AllObserver => {
                for observer in &self.observers {
                    if let Some(observer) = observer {
                        observer.stoc_sender.send(message.clone()).ok();
                    }
                }
            }
            SendTarget::None => {}
        }
    }

    pub fn send_game_message(&self, message: gm::Message, target: SendTarget) {
        // todo: run mask.
        self.send(stoc::Message::GameMessage(stoc::GameMessage { message }), target);
    }

    pub fn refresh_zone(&self, player: CorePlayer, location: Location) {
        for message in refresh_zone(self, player, location) {
            self.send_game_message(message, SendTarget::All);
        }
    }

    pub fn refresh_single(&self, player: CorePlayer, location: Location, sequence: i8) {
        let message = refresh_single(self, player, location, sequence as i32);
        self.send_game_message(message, SendTarget::All);
    }

    pub fn start_timer(&mut self) {
        let sender = self.request_sender.clone();
        self.timer_task = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if sender.send(Request::TimerTick).is_err() { break; }
            }
        }));
    }
    
    pub fn shuffle_deck(&mut self) {
        if let Some(deck) = self.players[0].as_mut().map(|p| &mut p.deck) {
            self.duel.shuffle_deck(&mut deck.main);
        }
        if let Some(deck) = self.players[1].as_mut().map(|p| &mut p.deck) {
            self.duel.shuffle_deck(&mut deck.main);
        }
    }
}

impl Deref for SingleDuel {
    type Target = common::Duel;
    fn deref(&self) -> &Self::Target { &self.duel }
}

impl DerefMut for SingleDuel {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.duel }
}

impl AsRef<common::Duel> for SingleDuel {
    fn as_ref(&self) -> &common::Duel { &self.duel }
}

impl AsMut<common::Duel> for SingleDuel {
    fn as_mut(&mut self) -> &mut common::Duel { &mut self.duel }
}

impl FromRequest<common::Request, State<SingleDuel>, Response> for &mut SingleDuel {
    fn from_request(bundle: &mut Bundle<common::Request, State<SingleDuel>, Response>) -> Option<Self> {
        Some(unsafe { &mut *(&mut bundle.state.duel as *mut SingleDuel) })
    }
}

pub struct SingleDuelHost {
    ctos_sender: mpsc::UnboundedSender<Request>,
}

impl SingleDuelHost {
    pub fn new(is_match: bool, seed_generator: Option<fn(u8) -> DuelSeed>) -> (Self, tokio::task::JoinHandle<()>) {
        let single_duel = SingleDuel::new(is_match, seed_generator);
        let request_sender = single_duel.request_sender.clone();
        let handle = single_duel.run().expect("duel already started");
        (Self { ctos_sender: request_sender }, handle)
    }
}

impl RoomProvider<ctos::Message, stoc::Message> for SingleDuelHost {
    type ServerToClientStream = UnboundedReceiverStream<stoc::Message>;

    fn add(&mut self, client_to_server_stream: impl Stream<Item = ctos::Message> + Unpin + Send + 'static) -> Self::ServerToClientStream {
        let ctos_sender = self.ctos_sender.clone();
        let (stoc_sender, stoc_receiver) = mpsc::unbounded_channel();
        let (return_sender, return_receiver) = mpsc::unbounded_channel();
        ctos_sender.send(Request::Join { stoc_sender }).ok();
        
        tokio::spawn(async move {
            let mut ctos_stream = Box::pin(client_to_server_stream);
            let mut stoc_stream = UnboundedReceiverStream::new(stoc_receiver);
            let mut my_position: Netplayer = Netplayer::Unknown;
            loop {
                tokio::select! {
                    message = ctos_stream.next() => {
                        let message = match message {
                            Some(message) => message,
                            None => ctos::Message::LeaveGame(ctos::LeaveGame)
                        };
                        ctos_sender.send(Request::Message(common::Request { message, extra: my_position })).ok();
                    }
                    message = stoc_stream.next() => {
                        if let Some(message) = message {
                            match &message {
                                stoc::Message::TypeChange(type_change) => my_position = type_change.player,
                                stoc::Message::LeaveGame(leave_game) => if leave_game.pos == my_position { break },
                                _ => ()
                            };
                            return_sender.send(message).ok();
                        } else {
                            break;
                        }
                    }
                }
            }
        });
        UnboundedReceiverStream::new(return_receiver)
    }
} 

mod ygopro_handlers {
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::OnceLock;

    use arc_swap::ArcSwap;
    use binrw::BinWrite;
    use linkme::distributed_slice;

    use log::warn;
    use ygopro_data::constants::*;
    use ygopro_data::data::DuelOptions;
    use ygopro_data::message::{ctos, stoc, gm};
    use ygopro_derive::handler;
    use ygopro_derive::register_to;
    use ygopro_handler::Bundle;
    use ygopro_handler::Processor;

    use crate::common;
    use crate::common::Response;
    use crate::common::SendTarget;
    use crate::managers::*;
    use crate::single_duel::PlayerIndex;
    use crate::single_duel::SingleDuel;
    use crate::single_duel::refresh_all;
    use crate::single_duel::refresh_zone;

    pub type Request = common::Request;
    pub type State = common::State<SingleDuel>;
    pub type Handler = common::Handler<SingleDuel>;

    impl ygopro_handler::FromRequest<Request, State, Response> for &mut common::Request {
        fn from_request(bundle: &mut Bundle<Request, State, Response>) -> Option<Self> {
            Some(unsafe { &mut *(&mut bundle.request as *mut common::Request) })
        }
    }

    #[distributed_slice]
    pub static YGOPRO_HANDLERS: [fn() -> (u8, Handler)];
    pub static YGOPRO_PROCESSOR: OnceLock<ArcSwap<Processor<u8, Request, State, Response, Handler>>> = OnceLock::new();
    pub fn reset_processor() -> &'static ArcSwap<Processor<u8, Request, State, Response, Handler>> {
        YGOPRO_PROCESSOR.get_or_init(|| {
            let mut processor = Processor::new();
            for build in YGOPRO_HANDLERS.iter() {
                let (key, handler) = build();
                processor.register(key, handler);
            }
            processor.resolve();
            ArcSwap::from(Arc::new(processor))
        })
    }

    #[handler(ctos::Response)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_response(duel: &mut SingleDuel, player: PlayerIndex, response: &ctos::Response) {
        let mut data = vec![];
        response.write_le(&mut Cursor::new(&mut data)).ok();
        duel.set_responseb(&data);
        if let Some(duel_player) = duel.get_player_mut_index(player) {
            duel_player.state = Some(ctos::MessageType::LeaveGame);
        }
        duel.request_sender.send(super::Request::Evolve).ok();
    }

    #[handler(ctos::HandResult)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_hand_result(duel: &mut SingleDuel, player: PlayerIndex, hand_result: &ctos::HandResult) {
        if let Some(duel_player) = duel.get_player_mut_index(player) {
            duel_player.hand = Some(hand_result.res);
        }
        let (message, winner) = {
            let (player1, player2) = duel.players.split_at_mut(1);
            let player1 = match player1[0].as_mut() { Some(p) => p, None => return };
            let player2 = match player2[0].as_mut() { Some(p) => p, None => return };
            let hand1 = match player1.hand { Some(res) => res, None => return };
            let hand2 = match player2.hand { Some(res) => res, None => return };
            let observer_message = stoc::HandResult { hand1, hand2 };
            let result = observer_message.judge();
            match result {
                HandResult::Draw => {
                    player1.hand = None;
                    player2.hand = None;
                    player1.state = Some(ctos::MessageType::HandResult);
                    player2.state = Some(ctos::MessageType::HandResult);
                    duel.send(stoc::SelectHand.into(), SendTarget::AllPlayer);
                    (observer_message, None)
                },
                HandResult::Win => {
                    player1.state = Some(ctos::MessageType::TpResult);
                    player2.state = Some(ctos::MessageType::LeaveGame);
                    (observer_message, Some(PlayerIndex::Player1))
                },
                HandResult::Lose => {
                    player1.state = Some(ctos::MessageType::LeaveGame);
                    player2.state = Some(ctos::MessageType::TpResult);
                    (observer_message, Some(PlayerIndex::Player2))
                }
            }
        };
        
        duel.send(message.swap_clone().into(), SendTarget::Single(Netplayer::Player(1)));
        duel.send(message.into(), SendTarget::Except(Netplayer::Player(1)));
        if let Some(winner) = winner {
            duel.send(stoc::SelectTp.into(), SendTarget::Single(winner.into()));
        }
        duel.stage = DuelStage::Firstgo;
    }

    #[handler(ctos::TpResult)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_tp_result(duel: &mut SingleDuel, player: PlayerIndex, tp_result: &ctos::TpResult) {
        duel.stage = DuelStage::Dueling;
        duel.first_attack_player = Some(if tp_result.result == CorePlayer::FirstAttackPlayer { player } else { player.opponent() });
        duel.set_player_info(CorePlayer::FirstAttackPlayer,  duel.host_info.start_lp as i32, duel.host_info.start_hand as i32, duel.host_info.draw_count as i32);
        duel.set_player_info(CorePlayer::SecondAttackPlayer, duel.host_info.start_lp as i32, duel.host_info.start_hand as i32, duel.host_info.draw_count as i32);
        duel.shuffle_deck();
        let mut player1 = match duel.players[0].as_ref() { Some(p) => p, None => return };
        let mut player2 = match duel.players[1].as_ref() { Some(p) => p, None => return };
        if (tp_result.result == CorePlayer::FirstAttackPlayer && player == PlayerIndex::Player2)
            || (tp_result.result == CorePlayer::SecondAttackPlayer && player == PlayerIndex::Player1) {
            std::mem::swap(&mut player1, &mut player2);
        }
        for &code in player1.deck.main.iter().rev() {
            duel.new_card(code, CorePlayer::FirstAttackPlayer, CorePlayer::FirstAttackPlayer, Location::Deck, 0, Position::FacedownDefense);
    }
        for &code in player1.deck.extra.iter().rev() {
            duel.new_card(code, CorePlayer::FirstAttackPlayer, CorePlayer::FirstAttackPlayer, Location::Extra, 0, Position::FacedownDefense);
        }
        for &code in player2.deck.main.iter().rev() {
            duel.new_card(code, CorePlayer::SecondAttackPlayer, CorePlayer::SecondAttackPlayer, Location::Deck, 0, Position::FacedownDefense);
        }
        for &code in player2.deck.extra.iter().rev() {
            duel.new_card(code, CorePlayer::SecondAttackPlayer, CorePlayer::SecondAttackPlayer, Location::Extra, 0, Position::FacedownDefense);
        }
        let deck1 = duel.query_field_count(CorePlayer::FirstAttackPlayer, Location::Deck) as u16;
        let extra1 = duel.query_field_count(CorePlayer::FirstAttackPlayer, Location::Extra) as u16;
        let deck2 = duel.query_field_count(CorePlayer::SecondAttackPlayer, Location::Deck) as u16;
        let extra2 = duel.query_field_count(CorePlayer::SecondAttackPlayer, Location::Extra) as u16;
        let start_lp = duel.host_info.start_lp as i32;
        let duel_rule = duel.host_info.duel_rule;
        let start = |player_type: u8| gm::Message::Start(gm::Start {
            player_type,
            rule: duel_rule,
            player1_lp: start_lp,
            player2_lp: start_lp,
            player1_deck_count: deck1,
            player1_extra_count: extra1,
            player2_deck_count: deck2,
            player2_extra_count: extra2,
        });
        duel.send(stoc::GameMessage { message: start(0) }.into(), SendTarget::Single(duel.to_net_player(CorePlayer::FirstAttackPlayer)));
        duel.send(stoc::GameMessage { message: start(1) }.into(), SendTarget::Single(duel.to_net_player(CorePlayer::SecondAttackPlayer)));
        let observer_player_type = match duel.first_attack_player {
            Some(PlayerIndex::Player1) => 0x10,
            Some(PlayerIndex::Player2) => 0x11,
            _ => unreachable!(),
        };
        duel.send(stoc::GameMessage { message: start(observer_player_type) }.into(), SendTarget::AllObserver);
        let extra0 = refresh_zone(duel, CorePlayer::FirstAttackPlayer, Location::Extra).pop().unwrap();
        let extra1 = refresh_zone(duel, CorePlayer::SecondAttackPlayer, Location::Extra).pop().unwrap();
        duel.send(stoc::GameMessage { message: extra0 }.into(), SendTarget::All);
        duel.send(stoc::GameMessage { message: extra1 }.into(), SendTarget::All);
        let mut options = DuelOptions::empty();
        if duel.host_info.no_shuffle_deck { options.insert(DuelOptions::PseudoShuffle); }
        duel.start(options, duel.host_info.duel_rule);
        let time_limit = duel.host_info.time_limit;
        if time_limit > 0 { 
            duel.time_elapsed = 0;
            let (player1, player2) = duel.players.split_at_mut(1);
            let player1 = match player1[0].as_mut() { Some(p) => p, None => return };
            let player2 = match player2[0].as_mut() { Some(p) => p, None => return };
            player1.time_limit = time_limit;
            player2.time_limit = time_limit;
            player1.time_backed = time_limit;
            player2.time_backed = time_limit;
            player1.time_compensator = time_limit;
            player2.time_compensator = time_limit;
            duel.start_timer();
        }
        duel.request_sender.send(super::Request::Evolve).ok();
    }

    #[handler(ctos::UpdateDeck)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_update_deck(duel: &mut SingleDuel, player: PlayerIndex, update_deck: &ctos::UpdateDeck) -> Option<stoc::Message> {
        let netplayer: Netplayer = player.into();
        if duel.get_player_index(player)?.ready {
            warn!("UpdateDeck requested but player is already ready");
            return None;
        }
        let mut deck = update_deck.deck.clone();
        if duel.duel_count == 0 {
            let data_manager = data_manager::load().clone().expect("unintied data manager");
            deck.load(|code| data_manager.get_card(code));
            duel.get_player_mut_index(player)?.deck = deck;
        } else {
            let data_manager = data_manager::load().clone().expect("unintied data manager");
            let side_check_result = duel.get_player_index(player)?.deck.check_after_replacing_side(&mut deck, |code| data_manager.get_card(code));
            if let Err(_error) = side_check_result {
                return Some(stoc::ErrorMessage { err: ErrorMessage::SideError }.into());
            }
            if let Some(player) = duel.get_player_mut_index(player) {
                player.deck = deck;
                player.ready = true;
            }
            duel.send(stoc::DuelStart.into(), netplayer.into());
            let ready = {
                let player1 = duel.players[0].as_ref()?;
                let player2 = duel.players[1].as_ref()?;
                player1.ready && player2.ready
            };
            if ready {
                let decider = duel.first_attack_decider.unwrap_or(PlayerIndex::Player1);
                duel.send(stoc::SelectTp.into(), decider.into());
                duel.get_player_mut_index(decider)?.state = Some(ctos::MessageType::TpResult);
                duel.get_player_mut_index(decider.opponent())?.state = Some(ctos::MessageType::LeaveGame);
                duel.stage = DuelStage::Firstgo; 
            }
        }
        None
    }

    #[handler(ctos::CreateGame)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_create_game(duel: &mut SingleDuel, create_game: &ctos::CreateGame) {
        duel.host_info = create_game.info.clone();
        duel.name = create_game.name.clone();
        duel.pass = create_game.pass.clone();
    }

    #[handler(ctos::JoinGame)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_join_game(duel: &mut SingleDuel, request: &mut common::Request, join_game: &ctos::JoinGame) -> Result<Vec<stoc::Message>, stoc::Message> {
        if join_game.version != crate::PRO_VERSION {
            return Err(stoc::ErrorMessage { err: ErrorMessage::VersionError(crate::PRO_VERSION) }.into());
        }
        if join_game.pass != duel.pass {
            return Err(stoc::ErrorMessage { err: ErrorMessage::JoinError(JoinError::WrongPassword) }.into());
        }
        let mut response_messages = vec![];

        // calculate current user position
        let is_creator = duel.players[0].is_none() && duel.players[1].is_none() && duel.observers.is_empty();
        let mut observer_count = duel.observer_count();
        let pos = if duel.players[0].is_none() {
            Netplayer::Player(0)
        } else if duel.players[1].is_none() {
            Netplayer::Player(1)
        } else {
            let observer_index = duel.observers.iter().position(|v| v.is_none()).unwrap_or(duel.observers.len()) as u8;
            observer_count = observer_count + 1;
            Netplayer::Observer(observer_index)
        };
        request.extra = pos;
        if is_creator { duel.host_player = pos; }
        
        response_messages.push(stoc::JoinGame{ info: duel.host_info.clone() }.into());
        response_messages.push(stoc::TypeChange{ 
            player: pos,
            host: is_creator
        }.into());
        
        // broadcast player change
        let player = duel.last_init_player.take().expect("cannot get init player when join game");
        if matches!(pos, Netplayer::Observer(_)) {
            duel.send(stoc::HsWatchChange { watch_count: observer_count }.into(), SendTarget::All);
        } else {
            duel.send(stoc::HsPlayerEnter { name: player.name.clone(), pos }.into(), SendTarget::All);
        }

        // actual player change
        match pos {
            Netplayer::Observer(index) => {
                if index as usize >= duel.observers.len() { duel.observers.push(Some(player)); } 
                else { duel.observers[index as usize] = Some(player); }
            }
            Netplayer::Player(0) => { duel.players[0] = Some(player.into()); }
            Netplayer::Player(1) => { duel.players[1] = Some(player.into()); }
            _ => warn!("try to put into an illegal player pos")
        };

        // tell current user now how room is now.
        for i in [0u8, 1u8] {
            if let Some(player) = duel.players[i as usize].as_ref() {
                response_messages.push(stoc::HsPlayerEnter { name: player.name.clone(), pos: Netplayer::Player(i) }.into());
                if player.ready { response_messages.push(stoc::HsPlayerChange { status: PlayerChange::new()
                    .with_player(Netplayer::Player(i))
                    .with_state(PlayerChangeState::Ready)
                }.into()); }
            }
        };
        if observer_count > 0 {
            response_messages.push(stoc::HsWatchChange{ watch_count: observer_count }.into());
        }

        Ok(response_messages)
    }

    #[handler(ctos::HsToDuelist)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_hs_to_duelist(duel: &mut SingleDuel, request: &mut common::Request, player: Netplayer) -> Option<stoc::Message> {
        let observer_index = if let Netplayer::Observer(observer_index) = player {
            observer_index as usize
        } else {
            warn!("HsToDuelist requested by non-observer");
            return None;
        };
        if duel.players[0].is_some() && duel.players[1].is_some() {
            warn!("HsToDuelist requested but both player slots are full");
            return None;
        }
        let Some(observer) = duel.observers[observer_index].take() else {
            warn!("try to convert observer to player but observer dont exist");
            return None;
        };
        let i_am_host = duel.host_player == player;
        let new_position_index = if duel.players[0].is_none() { 0 } else { 1 };
        let new_position = Netplayer::Player(new_position_index as u8);
        request.extra = new_position;
        if i_am_host { duel.host_player = new_position; }
        let name = observer.name.clone();
        duel.players[new_position_index] = Some(observer.into());
        duel.send(stoc::HsPlayerEnter { name, pos: new_position }.into(), SendTarget::All);
        duel.send(stoc::HsWatchChange { watch_count: duel.observer_count() }.into(), SendTarget::All);
        Some(stoc::TypeChange {
            player: new_position,
            host: i_am_host
        }.into())
    }

    #[handler(ctos::HsToObserver)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_hs_to_observer(duel: &mut SingleDuel, request: &mut common::Request, player: PlayerIndex) -> Option<stoc::Message> {
        let original_netplayer: Netplayer = player.into();
        let position = player as u8 as usize;
        let Some(duel_player) = duel.players[position].take() else {
            warn!("to_observer requested but player slot is empty");
            return None;
        };
        let current_netplayer = duel.insert_observer(duel_player.player);
        request.extra = current_netplayer;
        let i_am_host = duel.host_player == original_netplayer;
        if i_am_host { duel.host_player = current_netplayer }
        duel.send(stoc::HsPlayerChange { 
            status: PlayerChange::new()
                .with_state(PlayerChangeState::Observe)
                .with_player(original_netplayer) 
        }.into(), SendTarget::All);
        Some(stoc::TypeChange {
            player: current_netplayer,
            host: i_am_host
        }.into())
    }

    #[handler(ctos::LeaveGame)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_leave_game(duel: &mut SingleDuel, player: Netplayer) -> bool {
        if player == duel.host_player {
            let new_host: Netplayer = if duel.players[0].is_some() && player != Netplayer::Player(0) {
                Netplayer::Player(0)
            } else if duel.players[1].is_some() && player != Netplayer::Player(1) {
                Netplayer::Player(1)
            } else {
                duel.end();
                return true;
            };
            duel.host_player = new_host;
            if duel.stage == DuelStage::Begin {
                if let Some(player) = duel.get_player_mut(new_host) {
                    player.ready = false;
                }
                duel.send(stoc::TypeChange {
                    player: new_host,
                    host: true
                }.into(), SendTarget::Single(new_host));
            }
        }

        match player {
            Netplayer::Observer(observer_index) => {
                let index = observer_index as usize;
                duel.observers[index] = None;
                if duel.stage == DuelStage::Begin {
                    let observer_count = duel.observer_count();
                    duel.send(stoc::HsWatchChange { watch_count: observer_count }.into(), SendTarget::All);
                }
            }
            Netplayer::Player(leaving_netplayer) => {
                if duel.stage == DuelStage::Begin {
                    duel.players[leaving_netplayer as usize] = None;
                    let leave_message: stoc::Message = stoc::HsPlayerChange { status: PlayerChange::new()
                        .with_state(PlayerChangeState::Leave)
                        .with_player(player)
                    }.into();
                    duel.send(leave_message, SendTarget::All);
                } else {
                    if duel.stage == DuelStage::Siding {
                        duel.send(stoc::DuelStart.into(), SendTarget::AllPlayer);
                    }
                    if duel.stage != DuelStage::End {
                        let leaving_index = if leaving_netplayer == 0 { PlayerIndex::Player1 } else { PlayerIndex::Player2 };
                        let winner = duel.to_core_player(leaving_index).opponent();
                        let win_message = gm::Message::Win(gm::Win {winner, reason: WinReason::OpponentLeave});
                        duel.send(stoc::GameMessage { message: win_message }.into(), SendTarget::All);
                        duel.send(stoc::DuelEnd.into(), SendTarget::All);
                        duel.end();
                        duel.players[leaving_netplayer as usize] = None;
                        return true;
                    }
                }
                duel.players[leaving_netplayer as usize] = None;
            }
            Netplayer::Unknown => {}
        }
        false
    }

    #[handler(ctos::HsStart)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_hs_start(duel: &mut SingleDuel, player: Netplayer) {
        if player != duel.host_player {
            warn!("HsStart requested by non-host");
            return;
        }

        let (deck1_main, deck1_side, deck1_extra, deck2_main, deck2_side, deck2_extra) = {
            let player1 = match duel.players[0].as_ref() { Some(p) => p, None => { warn!("HsStart: player1 missing"); return; } };
            let player2 = match duel.players[1].as_ref() { Some(p) => p, None => { warn!("HsStart: player2 missing"); return; } };
            if !player1.ready || !player2.ready {
                warn!("HsStart: not all players ready");
                return;
            }
            (
                player1.deck.main.len() as u16,
                player1.deck.side.len() as u16,
                player1.deck.extra.len() as u16,
                player2.deck.main.len() as u16,
                player2.deck.side.len() as u16,
                player2.deck.extra.len() as u16,
            )
        };

        duel.send(stoc::DuelStart.into(), SendTarget::All);

        let player1_count = stoc::DeckCount {
            mainc_s: deck1_main, sidec_s: deck1_side, extrac_s: deck1_extra,
            mainc_o: deck2_main, sidec_o: deck2_side, extrac_o: deck2_extra,
        };
        let player2_count = stoc::DeckCount {
            mainc_s: deck2_main, sidec_s: deck2_side, extrac_s: deck2_extra,
            mainc_o: deck1_main, sidec_o: deck1_side, extrac_o: deck1_extra,
        };
        duel.send(player1_count.into(), SendTarget::Single(Netplayer::Player(0)));
        duel.send(player2_count.into(), SendTarget::Single(Netplayer::Player(1)));

        duel.send(stoc::SelectHand.into(), SendTarget::AllPlayer);

        let (player1, player2) = duel.players.split_at_mut(1);
        let player1 = player1[0].as_mut().unwrap();
        let player2 = player2[0].as_mut().unwrap();
        player1.hand = None;
        player2.hand = None;
        player1.state = Some(ctos::MessageType::HandResult);
        player2.state = Some(ctos::MessageType::HandResult);
        duel.stage = DuelStage::Finger;
    }

    #[handler(ctos::Surrender)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_surrender(duel: &mut SingleDuel, index: PlayerIndex) {
        if duel.stage != DuelStage::Dueling {
            warn!("Surrender requested but not in dueling stage");
            return;
        }
        let core_surrendering = duel.to_core_player(index);
        duel.win_and_end(core_surrendering, WinReason::OpponentSurrender);
    }

    #[handler(ctos::TimeConfirm)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_time_confirm(duel: &mut SingleDuel, index: PlayerIndex) {
        if duel.host_info.time_limit == 0 { return; }
        if Some(index) != duel.last_response {
            warn!("TimeConfirm requested by wrong player");
            return;
        }
        let time_elapsed = duel.time_elapsed;
        let Some(duel_player) = duel.get_player_mut_index(index) else {
            warn!("TimeConfirm requested but player slot is empty");
            return;
        };
        duel_player.state = Some(ctos::MessageType::Response);
        if time_elapsed < 10 && time_elapsed <= duel_player.time_compensator {
            duel_player.time_compensator -= time_elapsed;
        } else {
            duel_player.time_limit = duel_player.time_limit.saturating_sub(time_elapsed);
        }
        duel.time_elapsed = 0;
    }

    #[handler(ctos::Chat)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_chat(duel: &mut SingleDuel, player: Netplayer, chat: &ctos::Chat) {
        let chat = stoc::Chat {
            player: player.into(),
            msg: chat.msg.clone()
        };
        duel.send(chat.into(), SendTarget::All);
    }

    #[handler(ctos::PlayerInfo)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_player_info(duel: &mut SingleDuel, player_info: &ctos::PlayerInfo) {
        if let Some(player) = duel.last_init_player.as_mut() {
            player.name = player_info.name.clone();
        } else {
            warn!("We receive a player_info, but no user is waiiting init.");
        }
    }

    #[handler(ctos::HsReady)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_hs_ready(duel: &mut SingleDuel, index: PlayerIndex) -> Vec<stoc::Message> {
        let netplayer: Netplayer = index.into();
        if duel.stage != DuelStage::Begin {
            warn!("HsReady requested outside Begin stage");
            return vec![];
        }
        let no_check_deck = duel.host_info.no_check_deck;
        let lflist_index = duel.host_info.lflist;
        let rule = duel.host_info.rule;
        let Some(duel_player) = duel.get_player_mut_index(index) else {
            warn!("HsReady requested by non-player");
            return vec![];
        };
        if duel_player.ready {
            warn!("HsReady requested but player is already ready");
            return vec![];
        }
        if !no_check_deck {
            let deck_manager = deck_manager::load();
            let data_manager = data_manager::load();
            let lflist = deck_manager.as_ref().and_then(|dm| dm.get_lflist(lflist_index));
            let (Some(lflist), Some(data_manager)) = (lflist, data_manager.as_ref()) else { return vec![]; };
            if let Err(deck_error) = duel_player.deck.prepare(&lflist, rule, |code| data_manager.get_card(code)) {
                duel.send(stoc::HsPlayerChange { status: PlayerChange::new()
                    .with_state(PlayerChangeState::Notready)
                    .with_player(netplayer)
                }.into(), SendTarget::Single(netplayer));
                duel.send(stoc::ErrorMessage { err: ErrorMessage::DeckError(deck_error) }.into(), SendTarget::Single(netplayer));
                return vec![];
            }
        }
        duel_player.ready = true;
        duel.send(stoc::HsPlayerChange {
            status: PlayerChange::new()
                .with_state(PlayerChangeState::Ready)
                .with_player(netplayer)
        }.into(), SendTarget::All);
        vec![]
    }

    #[handler(ctos::HsNotReady)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_hs_not_ready(duel: &mut SingleDuel, index: PlayerIndex) {
        if duel.stage != DuelStage::Begin { 
            warn!("HsNotReady requested outside Begin stage"); 
            return; 
        }
        let Some(duel_player) = duel.get_player_mut_index(index) else {
            warn!("HsNotReady requested by non-player");
            return;
        };
        if !duel_player.ready { 
            warn!("HsNotReady requested but player is already not ready"); 
            return 
        }
        duel_player.ready = false;
        let netplayer: Netplayer = index.into();
        duel.send(stoc::HsPlayerChange { 
            status: PlayerChange::new()
                .with_state(PlayerChangeState::Notready)
                .with_player(netplayer) 
        }.into(), SendTarget::All);
    }

    #[handler(ctos::HsKick)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_hs_kick(duel: &mut SingleDuel, kicker: Netplayer, kick: &ctos::HsKick) {
        if kicker != duel.host_player {
            warn!("HsKick requested by non-host");
            return;
        }
        if duel.stage != DuelStage::Begin {
            warn!("HsKick requested outside Begin stage");
            return;
        }
        let Netplayer::Player(target) = kick.pos else {
            warn!("HsKick requested to kick non-player");
            return;
        };
        if kicker == kick.pos {
            warn!("HsKick: cannot kick self");
            return;
        }
        if duel.players[target as usize].is_none() {
            warn!("HsKick: target slot empty");
            return;
        }
        duel.players[target as usize] = None;
        duel.send(stoc::HsPlayerChange {
            status: PlayerChange::new()
                .with_state(PlayerChangeState::Leave)
                .with_player(kick.pos)
        }.into(), SendTarget::All);
    }

    #[handler(ctos::RequestField)]
    #[register_to(YGOPRO_HANDLERS)]
    fn on_request_field(duel: &mut SingleDuel, player: PlayerIndex) -> Vec<stoc::Message> {
        let mut messages = vec![];
        messages.push(stoc::DuelStart.into());

        let player_type: u8 = player as u8;
        let deck0 = duel.query_field_count(CorePlayer::FirstAttackPlayer, Location::Deck) as u16;
        let extra0 = duel.query_field_count(CorePlayer::FirstAttackPlayer, Location::Extra) as u16;
        let deck1 = duel.query_field_count(CorePlayer::SecondAttackPlayer, Location::Deck) as u16;
        let extra1 = duel.query_field_count(CorePlayer::SecondAttackPlayer, Location::Extra) as u16;
        let start_lp = duel.host_info.start_lp as i32;
        messages.push(stoc::GameMessage {
            message: gm::Message::Start(gm::Start {
                player_type,
                rule: duel.host_info.duel_rule,
                player1_lp: start_lp,
                player2_lp: start_lp,
                player1_deck_count: deck0,
                player1_extra_count: extra0,
                player2_deck_count: deck1,
                player2_extra_count: extra1,
            })
        }.into());

        // todo: turn_player → send 2 MSG_NEW_TURN when turn_player == 1
        messages.push(stoc::GameMessage {
            message: gm::Message::NewTurn(gm::NewTurn {
                player: CorePlayer::FirstAttackPlayer,
            })
        }.into());

        messages.push(stoc::GameMessage {
            message: gm::Message::NewPhase(gm::NewPhase {
                phase: duel.phase,
            })
        }.into());

        // todo: query_field_info

        for gm_message in refresh_all(duel) {
            messages.push(stoc::GameMessage { message: gm_message }.into());
        }

        if duel.deck_reversed {
            messages.push(stoc::GameMessage { message: gm::Message::ReverseDeck(gm::ReverseDeck) }.into());
        }
        // todo: MSG_DECK_TOP for both players (query_field_card deck top, check faceup/reversed)

        for index in [0, 1] {
            messages.push(stoc::TimeLimit {
                player: duel.to_core_player(PlayerIndex::try_from(index).unwrap()),
                left_time: duel.players[index as usize].as_ref().map_or(0, |p| p.time_limit),
            }.into());
        }

        messages.push(stoc::FieldFinish.into());
        messages
    }
}

fn query(duel: &SingleDuel, player: CorePlayer, location: Location, query_flag: Query) -> gm::Message {
    let mut buffer = [0; 0x40000];
    let data_size = duel.duel.query_field_card(player, location, query_flag, &mut buffer, false) as usize;
    let mut cursor = Cursor::new(&buffer[..data_size]);
    let cards: Vec<UpdateCardInfo> = (0..).map_while(|_| UpdateCardInfo::read_le(&mut cursor).ok()).collect();
    gm::UpdateData { player, location, data: cards }.into()
}

fn refresh_zone(duel: &SingleDuel, core_player: CorePlayer, location: Location) -> Vec<gm::Message> {
    let mut messages = Vec::new();
    let players: &[CorePlayer] = if core_player == CorePlayer::All {
        &[CorePlayer::FirstAttackPlayer, CorePlayer::SecondAttackPlayer]
    } else {
        std::slice::from_ref(&core_player)
    };
    for &player in players {
        for loc in [Location::MZone, Location::SZone, Location::Hand, Location::Extra, Location::Grave, Location::Removed].iter() {
            if location.intersects(*loc) {
                messages.push(query(duel, player, *loc, Query::all()));
            }
        }
    }
    messages
}

fn refresh_all(duel: &SingleDuel) -> Vec<gm::Message> {
    let mut messages = Vec::new();
    for &player in &[CorePlayer::FirstAttackPlayer, CorePlayer::SecondAttackPlayer] {
        for location in [Location::MZone, Location::SZone, Location::Hand, Location::Extra, Location::Grave, Location::Removed] {
            messages.push(query(duel, player, location, Query::all()));
        }
    }
    messages
}

fn refresh_single(duel: &SingleDuel, player: CorePlayer, location: Location, sequence: i32) -> gm::Message {
    let mut buffer = [0u8; 0x40000];
    let len = duel.query_card(player, location, sequence as u8, Query::all(), &mut buffer, false) as usize;
    let mut cursor = Cursor::new(&buffer[..len]);
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

mod ygocore_handlers {
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::OnceLock;

    use arc_swap::ArcSwap;
    use binrw::BinRead;
    use linkme::distributed_slice;
    
    use log::warn;
use ygopro_core_wrapper::ProcessResultFlags;
    use ygopro_data::constants::CorePlayer;
    use ygopro_data::constants::Hint;
    use ygopro_data::constants::Location;
    use ygopro_data::constants::Netplayer;
    use ygopro_data::message::ctos;
    use ygopro_data::message::gm;
    use ygopro_data::message::stoc;
    use ygopro_handler::Bundle;
    use ygopro_handler::FromRequest;
    use ygopro_handler::IntoResponse;
    use ygopro_handler::Processor;
    use ygopro_derive::handler;
    use ygopro_derive::register_to;

    use crate::common;
    use crate::common::SendTarget;
    use crate::single_duel::SingleDuel;

    pub type Request = ygopro_handler::extract::Request<gm::Message, Netplayer>; 
    pub type State = common::State<SingleDuel>;
    pub type Handler = ygopro_handler::sync_handler::SyncHandler<Request, State, Response>;

    pub struct Response {
        pub target: SendTarget,
        pub refresh: (CorePlayer, Location, i8)
    }

    impl Default for Response {
        fn default() -> Self {
            Self { target: SendTarget::All, refresh: (CorePlayer::None, Location::empty(), -1) }
        }
    }

    impl IntoResponse<Response> for () {
        fn into_response(self) -> Response {
            Default::default()
        }
    }

    impl FromRequest<Request, State, Response> for &mut SingleDuel {
        fn from_request(bundle: &mut Bundle<Request, State, Response>) -> Option<Self> {
            Some(unsafe { &mut *(&mut bundle.state.duel as *mut SingleDuel) })
        }
    }

    impl IntoResponse<Response> for SendTarget {
        fn into_response(self) -> Response {
            Response { target: self, refresh: (CorePlayer::None, Location::empty(), -1) }
        }
    }

    impl IntoResponse<Response> for (CorePlayer, Location) {
        fn into_response(self) -> Response {
            Response { target: SendTarget::All, refresh: (self.0, self.1, -1) }
        }
    }

    impl IntoResponse<Response> for (CorePlayer, Location, i8) {
        fn into_response(self) -> Response {
            Response { target: SendTarget::All, refresh: self }
        }
    }

    #[distributed_slice]
    pub static YGOCORE_HANDLERS: [fn() -> (u8, Handler)];
    pub static YGOCORE_PROCESSOR: OnceLock<ArcSwap<Processor<u8, Request, common::State<SingleDuel>, Response, Handler>>> = OnceLock::new();
    pub fn reset_processor() -> &'static ArcSwap<Processor<u8, Request, common::State<SingleDuel>, Response, Handler>> {
        YGOCORE_PROCESSOR.get_or_init(|| {
            let mut processor = Processor::new();
            for build in YGOCORE_HANDLERS.iter() {
                let (key, handler) = build();
                processor.register(key, handler);
            }
            processor.resolve();
            ArcSwap::from(Arc::new(processor))
        })
    }

    /// process input messages, until waiting for user input or duel end.
    /// named `process` in original ygopro.
    pub fn evolve(duel: &mut SingleDuel) -> Vec<gm::Message> {
        let mut messages = vec![];
        loop {
            let result = duel.process();
            let engine_flag = result.flags();
            let engine_length = result.data_length() as usize;
            if engine_length > 0 {
                let mut buffer = vec![0u8; engine_length as usize];
                duel.get_message(&mut buffer);
                let mut cursor = Cursor::new(&buffer);
                while let Ok(message) = gm::Message::read_le(&mut cursor) {
                    messages.push(message);
                }
            }
            if engine_flag == ProcessResultFlags::End { break; }
            // if engine_flag == ProcessResultFlags::Waiting { break; }
            match gm::MessageType::from(&messages[messages.len() - 1]) {
                gm::MessageType::SelectBattleCommand 
                    | gm::MessageType::SelectIdleCommand
                    | gm::MessageType::SelectEffectYesNo
                    | gm::MessageType::SelectYesNo
                    | gm::MessageType::SelectCard
                    | gm::MessageType::SelectChain
                    | gm::MessageType::SelectPlace
                    | gm::MessageType::SelectPosition
                    | gm::MessageType::SelectTribute
                    | gm::MessageType::SelectCounter
                    | gm::MessageType::SelectSum
                    | gm::MessageType::SelectDisableField
                    | gm::MessageType::SortCard
                    | gm::MessageType::SelectUnselectCard => break,
                _ => continue
            };
        }
        messages
    }

    pub fn set_waiting(duel: &mut SingleDuel, player: CorePlayer) -> Option<()> {
        let index = match duel.to_player_index(player) {
            Some(player) => player,
            None => {
                warn!("try to set waiting to a non-sense plyaer: {:?}", player);
                return None
            }
        };
        duel.last_response = Some(index);
        duel.send(
            stoc::Message::GameMessage(stoc::GameMessage { message: gm::Message::Waiting(gm::Waiting) }),
            SendTarget::Single(index.opponent().into()),
        );
        if duel.host_info.time_limit > 0 {
            let time_limit: stoc::Message = stoc::TimeLimit { 
                player,
                left_time: duel.get_player_index(index)?.time_limit
            }.into();
            duel.send(time_limit.clone(), Netplayer::Player(0).into());
            duel.send(time_limit.clone(), Netplayer::Player(1).into());
            duel.get_player_mut_index(index)?.state = Some(ctos::MessageType::TimeConfirm);
        } else {
            duel.get_player_mut_index(index)?.state = Some(ctos::MessageType::Response);
        }
        Some(())
    }

    #[handler(gm::Retry)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_retry(duel: &mut SingleDuel, _message: &gm::Retry) -> SendTarget {
        let netplayer = match duel.last_response {
            Some(player_index) => duel.to_net_player(duel.to_core_player(player_index)),
            None => Netplayer::Unknown,
        };
        SendTarget::Single(netplayer)
    }

    #[handler(gm::Hint)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_hint(duel: &mut SingleDuel, message: &gm::Hint) -> SendTarget {
        match message._type {
            Hint::Event | Hint::Message | Hint::SelectMessage | Hint::Effect => {
                SendTarget::Single(duel.to_net_player(message.player))
            }
            Hint::OpponentSelected | Hint::Race | Hint::Attribute | Hint::Code | Hint::Number | Hint::Zone => {
                SendTarget::Except(duel.to_net_player(message.player))
            }
            Hint::Card => SendTarget::All,
        }
    }

    #[handler(gm::Win)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_win(duel: &mut SingleDuel, message: &gm::Win) -> SendTarget {
        // we need send win message before the duel end.
        duel.win_and_end(message.winner.opponent(), message.reason);
        SendTarget::None
    }

    #[handler(gm::SelectBattleCommand)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_battle_command(duel: &mut SingleDuel, message: &gm::SelectBattleCommand) -> SendTarget {
        duel.refresh_zone(CorePlayer::All, Location::MZone | Location::SZone | Location::Hand);
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectIdleCommand)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_idle_command(duel: &mut SingleDuel, message: &gm::SelectIdleCommand) -> SendTarget {
        duel.refresh_zone(CorePlayer::All, Location::MZone | Location::SZone | Location::Hand);
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectEffectYesNo)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_effect_yes_no(duel: &mut SingleDuel, message: &gm::SelectEffectYesNo) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectYesNo)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_yes_no(duel: &mut SingleDuel, message: &gm::SelectYesNo) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectOption)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_option(duel: &mut SingleDuel, message: &gm::SelectOption) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectCard)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_card(duel: &mut SingleDuel, message: &gm::SelectCard) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectChain)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_chain(duel: &mut SingleDuel, message: &gm::SelectChain) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectPlace)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_place(duel: &mut SingleDuel, message: &gm::SelectPlace) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectPosition)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_position(duel: &mut SingleDuel, message: &gm::SelectPosition) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectTribute)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_tribute(duel: &mut SingleDuel, message: &gm::SelectTribute) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectCounter)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_counter(duel: &mut SingleDuel, message: &gm::SelectCounter) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectSum)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_sum(duel: &mut SingleDuel, message: &gm::SelectSum) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SelectDisableField)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_disable_field(duel: &mut SingleDuel, message: &gm::SelectDisableField) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::SortCard)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_sort_card(duel: &mut SingleDuel, message: &gm::SortCard) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.player))
    }

    #[handler(gm::SelectUnselectCard)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_select_unselect_card(duel: &mut SingleDuel, message: &gm::SelectUnselectCard) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.selecting_player))
    }

    #[handler(gm::ConfirmCards)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_confirm_cards(duel: &mut SingleDuel, message: &gm::ConfirmCards) -> SendTarget {
        let is_deck = message.cards.first().map_or(false, |c| c.location == Location::Deck);
        if is_deck {
            SendTarget::Single(duel.to_net_player(message.player))
        } else {
            SendTarget::All
        }
    }

    #[handler(gm::ShuffleHand)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_shuffle_hand(_duel: &mut SingleDuel, message: &gm::ShuffleHand) -> (CorePlayer, Location) {
        (message.player, Location::Hand)
    }

    #[handler(gm::SwapGraveDeck)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_swap_grave_deck(_duel: &mut SingleDuel, message: &gm::SwapGraveDeck) -> (CorePlayer, Location) {
        (message.player, Location::Grave)
    }

    #[handler(gm::ShuffleSetCard)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_shuffle_set_card(_duel: &mut SingleDuel, message: &gm::ShuffleSetCard) -> (CorePlayer, Location) {
        (CorePlayer::All, message.location)
    }

    #[handler(gm::ReverseDeck)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_reverse_deck(duel: &mut SingleDuel, _message: &gm::ReverseDeck) {
        duel.deck_reversed = !duel.deck_reversed;
    }

    #[handler(gm::ShuffleExtra)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_shuffle_extra(_duel: &mut SingleDuel, message: &gm::ShuffleExtra) -> (CorePlayer, Location) {
        (message.player, Location::Extra)
    }

    #[handler(gm::NewTurn)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_new_turn(duel: &mut SingleDuel, _message: &gm::NewTurn) -> (CorePlayer, Location) {
        let time_limit = duel.host_info.time_limit;
        for duel_player in duel.players.iter_mut().flatten() {
            duel_player.time_limit = time_limit;
            duel_player.time_compensator = time_limit;
            duel_player.time_backed = time_limit;
        }
        (CorePlayer::All, Location::MZone | Location::SZone | Location::Hand)
    }

    #[handler(gm::NewPhase)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_new_phase(duel: &mut SingleDuel, message: &gm::NewPhase) -> (CorePlayer, Location) {
        duel.phase = message.phase;
        (CorePlayer::All, Location::MZone | Location::SZone | Location::Hand)
    }

    #[handler(gm::Move)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_move(_duel: &mut SingleDuel, message: &gm::Move) -> (CorePlayer, Location, i8) {
        let cc = message.current.0.controller;
        let cl = message.current.0.location;
        let cs = message.current.0.sequence;
        let pc = message.previous.0.controller;
        let pl = message.previous.0.location;
        if cl != Location::empty()
            && !cl.intersects(Location::Overlay)
            && (cl != pl || cc != pc)
        {
            (cc, cl, cs)
        } else {
            (CorePlayer::None, Location::empty(), -1)
        }
    }
    

    #[handler(gm::PositionChange)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_position_change(_duel: &mut SingleDuel, message: &gm::PositionChange) -> (CorePlayer, Location, i8) {
        if message.previous_position.is_face_down() && !message.current_position.is_face_down() {
            (message.controller, message.location, message.sequence)
        } else {
            (CorePlayer::None, Location::empty(), -1)
        }
    }

    #[handler(gm::Swap)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_swap(duel: &mut SingleDuel, message: &gm::Swap) -> SendTarget {
        let p1 = &message.position1.0;
        let p2 = &message.position2.0;
        duel.refresh_single(p1.controller, p1.location, p1.sequence);
        duel.refresh_single(p2.controller, p2.location, p2.sequence);
        SendTarget::All
    }

    #[handler(gm::Summoned)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_summoned(_duel: &mut SingleDuel, _message: &gm::Summoned) -> (CorePlayer, Location) {
        (CorePlayer::All, Location::MZone | Location::SZone)
    }

    #[handler(gm::SpecialSummoned)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_special_summoned(_duel: &mut SingleDuel, _message: &gm::SpecialSummoned) -> (CorePlayer, Location) {
        (CorePlayer::All, Location::MZone | Location::SZone)
    }

    #[handler(gm::FlipSummoning)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_flip_summoning(_duel: &mut SingleDuel, message: &gm::FlipSummoning) -> (CorePlayer, Location, i8) {
        let p = &message.position.0;
        (p.controller, p.location, p.sequence)
    }

    #[handler(gm::FlipSummoned)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_flip_summoned(_duel: &mut SingleDuel, _message: &gm::FlipSummoned) -> (CorePlayer, Location) {
        (CorePlayer::All, Location::MZone | Location::SZone)
    }

    #[handler(gm::Chained)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_chained(_duel: &mut SingleDuel, _message: &gm::Chained) -> (CorePlayer, Location) {
        (CorePlayer::All, Location::MZone | Location::SZone | Location::Hand)
    }

    #[handler(gm::ChainSolved)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_chain_solved(_duel: &mut SingleDuel, _message: &gm::ChainSolved) -> (CorePlayer, Location) {
        (CorePlayer::All, Location::MZone | Location::SZone | Location::Hand)
    }

    #[handler(gm::ChainEnd)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_chain_end(_duel: &mut SingleDuel, _message: &gm::ChainEnd) -> (CorePlayer, Location) {
        (CorePlayer::All, Location::MZone | Location::SZone | Location::Hand)
    }

    #[handler(gm::DamageStepStart)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_damage_step_start(_duel: &mut SingleDuel, _message: &gm::DamageStepStart) -> (CorePlayer, Location) {
        (CorePlayer::All, Location::MZone)
    }

    #[handler(gm::DamageStepEnd)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_damage_step_end(_duel: &mut SingleDuel, _message: &gm::DamageStepEnd) -> (CorePlayer, Location) {
        (CorePlayer::All, Location::MZone)
    }

    #[handler(gm::MissedEffect)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_missed_effect(duel: &mut SingleDuel, message: &gm::MissedEffect) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.player))
    }

    #[handler(gm::RockPaperScissors)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_rock_paper_scissors(duel: &mut SingleDuel, message: &gm::RockPaperScissors) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.player))
    }

    #[handler(gm::AnnounceRace)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_announce_race(duel: &mut SingleDuel, message: &gm::AnnounceRace) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.player))
    }

    #[handler(gm::AnnounceAttribute)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_announce_attribute(duel: &mut SingleDuel, message: &gm::AnnounceAttribute) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.player))
    }

    #[handler(gm::AnnounceCard)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_announce_card(duel: &mut SingleDuel, message: &gm::AnnounceCard) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.player))
    }

    #[handler(gm::AnnounceNumber)]
    #[register_to(YGOCORE_HANDLERS)]
    fn on_announce_number(duel: &mut SingleDuel, message: &gm::AnnounceNumber) -> SendTarget {
        SendTarget::Single(duel.to_net_player(message.player))
    }
}

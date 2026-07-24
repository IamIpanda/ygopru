use std::io::Cursor;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::OnceLock;

use log::warn;
use arc_swap::ArcSwap;
use binrw::BinRead;
use binrw::BinWrite;
use futures::Stream;
use linkme::distributed_slice;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use ygopro_core_wrapper::DuelSeed;
use ygopro_core_wrapper::ProcessResultFlags;
use ygopro_core_wrapper as core;
use ygopro_data::constants;
use ygopro_data::constants::*;
use ygopro_data::data::Deck;
use ygopro_data::data::DuelOptions;
use ygopro_data::data::Replay;
use ygopro_data::string::FixedLengthString;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_data::message::gm;
use ygopro_data::message::gm::Mask;
use ygopro_handler::Processor;
use ygopro_handler::RoomProvider;
use ygopro_handler::Bundle;
use ygopro_handler::FromRequest;
use ygopro_handler::MessageKey;
use ygopro_derive::handler;
use ygopro_derive::register_to;

use crate::common;
use crate::common::Response;
use crate::managers;

pub enum Request {
    Join { stoc_sender: mpsc::UnboundedSender<stoc::Message> },
    Message(common::Request),
    TimerTick,
}

type State = common::State<SingleDuel>;
type Handler = common::Handler<SingleDuel>;
type HandlerKey = u8;

#[distributed_slice]
pub static HANDLER_INFOS: [fn() -> (u8, Handler)];
static YGOPRO_PROCESSOR: OnceLock<ArcSwap<Processor<u8, common::Request, common::State<SingleDuel>, common::Response, Handler>>> = OnceLock::new();
pub fn reset_processor() -> &'static ArcSwap<Processor<u8, common::Request, common::State<SingleDuel>, common::Response, Handler>> {
    YGOPRO_PROCESSOR.get_or_init(|| {
        let mut processor = Processor::new();
        for build in HANDLER_INFOS.iter() {
            let (key, handler) = build();
            processor.register(key, handler);
        }
        processor.resolve();
        ArcSwap::from(Arc::new(processor))
    })
}

impl<Req, Res> FromRequest<Req, common::State<SingleDuel>, Res> for &mut SingleDuel where Req: Send, Res: Send {
    fn from_request(bundle: &mut Bundle<Req, common::State<SingleDuel>, Res>) -> Option<Self> {
        Some(unsafe { &mut *(&mut bundle.state.duel as *mut SingleDuel) })
    }
}

pub struct DuelPlayer {
    player: common::DuelPlayer,
    ready: bool,
    deck: Deck,
    hand_result: Option<Hand>,
    time_limit: u16,
    time_compensator: u16,
    time_backed: u16,
}

impl DuelPlayer {
    fn new(stoc_sender: mpsc::UnboundedSender<stoc::Message>) -> Self {
        Self {
            player: common::DuelPlayer {
                name: FixedLengthString::allocate(),
                state: None,
                stoc_sender,
            },
            ready: false,
            deck: Deck::new(),
            hand_result: None,
            time_limit: 0,
            time_compensator: 0,
            time_backed: 0,
        }
    }
}

impl From<common::DuelPlayer> for DuelPlayer {
    fn from(value: common::DuelPlayer) -> Self {
        Self {
            player: value, 
            ready: false,
            deck: Deck::new(),
            hand_result: None,
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

pub struct SingleDuel {
    duel: common::Duel,
    players: [Option<DuelPlayer>; 2], // indexed by NetPlayer
    first_attack_player: Netplayer,
    first_attack_decider: Netplayer,
    last_response: Netplayer,
    phase: Phase,
    deck_reversed: bool,
    is_match: bool,
    match_kill_card_code: i32,
    duel_count: u8,
    match_winner: Vec<Netplayer>,
    time_elapsed: u16,
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

impl FromRequest<common::Request, State, Response> for &mut DuelPlayer {
    fn from_request(bundle: &mut Bundle<common::Request, State, Response>) -> Option<Self> {
        let player = bundle.state.duel.get_player_mut(bundle.request.extra)?;
        Some(unsafe { &mut *(player as *mut DuelPlayer) })
    }
}

impl SingleDuel {
    pub fn new(is_match: bool, seed: DuelSeed) -> Self {
        let (observer_sender, observer_receiver) = broadcast::channel(128);
        let (request_sender, request_receiver) = mpsc::unbounded_channel();
        Self {
            duel: common::Duel {
                host_player: Netplayer::None,
                host_info: Default::default(),
                stage: DuelStage::Begin,
                duel: core::Duel::new(seed),
                name: FixedLengthString::allocate(),
                pass: FixedLengthString::allocate(),
            },
            players: [None, None],
            first_attack_player: Netplayer::None,
            last_response: Netplayer::None,
            turn_player: Netplayer::None,
            phase: Phase::Draw,
            deck_reversed: false,
            is_match,
            match_kill_card_code: 0,
            duel_count: 0,
            match_winner: Vec::new(),
            time_elapsed: 0,
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
            let processor = YGOPRO_PROCESSOR.get().expect("Processor not initialized").load_full();
            let mut duel_container = Some(self);

            while let Some(request) = stream.next().await {
                let mut duel = duel_container.take().expect("duel taken");
                match request {
                    Request::Join { stoc_sender } => {
                        if duel.last_init_player.is_some() { warn!("Two players are trying to init in the same duel.") }
                        duel.last_init_player = Some(common::DuelPlayer::new(stoc_sender));
                        duel_container = Some(duel);
                    }
                    Request::Message(request) => {
                        if let Some(player) = duel.get_player(request.extra) {
                            if let Some(message_type) = player.state {
                                if ctos::MessageType::from(&request.message) != message_type {
                                    warn!("Message type mismatch for player: {:?}", request.extra);
                                    duel_container = Some(duel);
                                    continue;
                                }
                            }
                        }
                        let position = request.extra;
                        let state = common::State { duel };
                        let bundle = Bundle { request, state, response: Default::default() };
                        let key = bundle.request.message_key();
                        let Bundle {
                            request,
                            state: common::State { mut duel },
                            response
                        } = processor.process_bundle(bundle, key).await;
                        match response {
                            ygopro_handler::extract::Response::Continue => {
                                todo!("dispatch original request to engine");
                            }
                            ygopro_handler::extract::Response::Replace(message) =>
                                duel.send(message, position),
                            ygopro_handler::extract::Response::ReplaceMultiple(messages) => {
                                for message in messages {
                                    duel.send(message, position);
                                }
                            }
                            ygopro_handler::extract::Response::Swallow => {}
                            ygopro_handler::extract::Response::Stop => { break; }
                            ygopro_handler::extract::Response::Kick => {
                                duel.send(stoc::Message::Kick(stoc::Kick), position);
                            }
                        }
                        duel_container = Some(duel);
                    }
                    Request::TimerTick => {
                        if duel.host_info.time_limit > 0 {
                            duel.time_elapsed = duel.time_elapsed.saturating_add(1);
                            let timed_out = {
                                let last_response = duel.last_response;
                                duel.players[last_response as usize].as_ref()
                                    .map_or(false, |player| duel.time_elapsed >= player.time_limit)
                            };
                            if timed_out {
                                let loser = duel.to_core_player(duel.last_response);
                                duel.win_and_end(loser, WinReason::OpponentSurrender);
                            }
                        }
                        duel_container = Some(duel);
                    }
                }
            }
        });

        Some(handle)
    }

    fn _send_to_player<Player>(message: stoc::Message, player: Option<&mut Player>)
    where Player: AsMut<common::DuelPlayer> {
        match player {
            Some(player) => { player.as_mut().stoc_sender.send(message).ok(); },
            None => warn!("Try to send message to a not exist user.")
        }
    }

    fn _send_to_netplayer(&mut self, message: stoc::Message, target: Netplayer) {
         match target {
            Netplayer::Player1 => SingleDuel::_send_to_player(message, self.players[0].as_mut()),
            Netplayer::Player2 => SingleDuel::_send_to_player(message, self.players[1].as_mut()),
            Netplayer::None => (),
            Netplayer::All => {
                SingleDuel::_send_to_player(message.clone(), self.players[0].as_mut());
                SingleDuel::_send_to_player(message, self.players[1].as_mut());
            },
            _ => warn!("Try to send message to a invalid player {:?}.", target)
        }
    }

    fn send(&mut self, message: stoc::Message, target: ExtendedNetplayer) {
        let masked = {
            let mut m = message.clone();
            if let stoc::Message::GameMessage(g) = &mut m {
                g.message.mask();
            }
            m
        };
        self.messages.push(message.clone());
        self.observer_messages.push(masked.clone());
        self.observer_sender.send(masked.clone()).ok();
        match target {
            ExtendedNetplayer::Player(netplayer) => {
                let mut m = message;
                if let stoc::Message::GameMessage(g) = &mut m {
                    g.message.mask_towards(self.to_core_player(netplayer));
                }
                self._send_to_netplayer(m, netplayer);
            }
            ExtendedNetplayer::Observer(index) => {
                SingleDuel::_send_to_player(masked, self.observers[index as usize].as_mut());
            }
            ExtendedNetplayer::Unknown => warn!("Try to send message to unknown user"),
            ExtendedNetplayer::None => (),
            ExtendedNetplayer::All => {
                let mut m0 = message.clone();
                let mut m1 = message.clone();
                if let stoc::Message::GameMessage(g) = &mut m0 {
                    g.message.mask_towards(self.to_core_player(Netplayer::Player1));
                }
                if let stoc::Message::GameMessage(g) = &mut m1 {
                    g.message.mask_towards(self.to_core_player(Netplayer::Player2));
                }
                SingleDuel::_send_to_player(m0, self.players[0].as_mut());
                SingleDuel::_send_to_player(m1, self.players[1].as_mut());
                for player in self.observers.iter_mut() {
                    SingleDuel::_send_to_player(masked.clone(), player.as_mut());
                }
            },
        }
    }

    pub fn subscribe_observer(&self) -> impl Stream<Item = stoc::Message> {
        let history = tokio_stream::iter(self.observer_messages.clone());
        let live = BroadcastStream::new(self.observer_sender.subscribe())
            .filter_map(|result| result.ok());
        history.chain(live)
    }

    pub fn get_player(&self, player: ExtendedNetplayer) -> Option<&DuelPlayer> {
        match player {
            ExtendedNetplayer::Player(netplayer) => {
                self.players[netplayer as usize].as_ref()
            }
            _ => None,
        }
    }

    pub fn get_player_mut(&mut self, player: ExtendedNetplayer) -> Option<&mut DuelPlayer> {
        match player {
            ExtendedNetplayer::Player(netplayer) => {
                self.players[netplayer as usize].as_mut()
            }
            _ => None,
        }
    }

    pub fn observer_count(&self) -> u16 {
        self.observers.iter().fold(0, |s, v| { s + if v.is_some(){ 1 } else { 0 } }) as u16
    }

    pub fn to_core_player(&self, player: Netplayer) -> CorePlayer {
        let core_player: CorePlayer = player.into();
        if self.first_attack_player == Netplayer::Player1 {
            core_player
        } else {
            core_player.opponent()
        }
    }

    pub fn to_player_index(&self, core_player: CorePlayer) -> Netplayer {
        match self.first_attack_player {
            Netplayer::Player1 => core_player.opponent().into(),
            _ => core_player.into()
        }
    }

    pub fn calculate_replay(&self) -> Replay {
        todo!()
    }

    pub fn win_and_end(&mut self, loser: CorePlayer, reason: WinReason) {
        let winner = loser.opponent();
        let win_message = gm::Message::Win(gm::Win { winner, reason });
        self.send(stoc::GameMessage { message: win_message }.into(), ExtendedNetplayer::All);
        if self.is_match {
            let winner_netplayer = self.to_player_index(winner);
            self.match_winner.push(winner_netplayer);
            self.duel_count += 1;
        }
        self.send(stoc::DuelEnd.into(), ExtendedNetplayer::All);
        self.end();
    }

    pub fn end(&mut self) {
        if let Some(timer_task) = self.timer_task.take() {
            timer_task.abort();
        }
        let replay = self.calculate_replay();
        self.send(stoc::Replay{ replay: Box::new(replay) }.into(), ExtendedNetplayer::All);
        self.duel.end();
    }
}

pub struct SingleDuelHost {
    ctos_sender: mpsc::UnboundedSender<Request>,
}

impl SingleDuelHost {
    pub fn new(is_match: bool, seed: DuelSeed) -> (Self, tokio::task::JoinHandle<()>) {
        let single_duel = SingleDuel::new(is_match, seed);
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
            let mut my_position: ExtendedNetplayer = ExtendedNetplayer::Unknown;
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
                                stoc::Message::TypeChange(type_change) => my_position = (&type_change.change).into(),
                                stoc::Message::Kick(_) => todo!(),
                                _ => ()
                            };
                            return_sender.send(message).ok();
                        }
                    }
                }
            }
        });
        UnboundedReceiverStream::new(return_receiver)
    }
}

// ============== CTOS Handlers ==============
#[handler(ctos::Response)]
#[register_to(HANDLER_INFOS)]
fn on_response(duel: &mut SingleDuel, response: &ctos::Response) -> Vec<stoc::Message> {
    let mut data = vec![];
    response.write_le(&mut Cursor::new(&mut data)).ok();
    duel.set_responseb(&data);
    let mut messages = vec![];

    loop {
        let result = duel.process();
        let engine_flag = result.flags();

        if engine_flag == ProcessResultFlags::End { break; }
        let engine_length = result.data_length() as usize;
        if engine_length > 0 {
            let mut buffer = vec![0u8; engine_length as usize];
            duel.get_message(&mut buffer);
            let mut cursor = Cursor::new(&buffer);
            if let Ok(message) = gm::Message::read_le(&mut cursor) {
                messages.push(stoc::Message::GameMessage(stoc::GameMessage { message }));
            }
        }
        if engine_flag == ProcessResultFlags::Waiting { break; }

    }
    vec![]
}

#[handler(ctos::HandResult)]
#[register_to(HANDLER_INFOS)]
fn on_hand_result(duel: &mut SingleDuel, player: ExtendedNetplayer, hand_result: &ctos::HandResult) {
    if let Some(player) = duel.get_player_mut(player) {
        player.hand_result = Some(hand_result.res);
    }
    let (message, winner) = {
        let (player1, player2) = duel.players.split_at_mut(1);
        let player1 = match player1[0].as_mut() { Some(p) => p, None => return };
        let player2 = match player2[0].as_mut() { Some(p) => p, None => return };
        let hand1 = match player1.hand_result { Some(res) => res, None => return };
        let hand2 = match player2.hand_result { Some(res) => res, None => return };
        let observer_message = stoc::HandResult { hand1, hand2 };
        let result = observer_message.judge();
        match result {
            constants::HandResult::Draw => {
                player1.hand_result = None;
                player2.hand_result = None;
                player1.state = Some(ctos::MessageType::HandResult);
                player2.state = Some(ctos::MessageType::HandResult);
                duel.send(stoc::SelectHand.into(), ExtendedNetplayer::Player(Netplayer::Player1));
                duel.send(stoc::SelectHand.into(), ExtendedNetplayer::Player(Netplayer::Player2));
                (observer_message, Netplayer::None)
            },
            constants::HandResult::Win => {
                player1.state = Some(ctos::MessageType::TpResult);
                player2.state = None;
                (observer_message, Netplayer::Player1)
            },
            constants::HandResult::Lose => {
                player1.state = None;
                player2.state = Some(ctos::MessageType::TpResult);
                (observer_message, Netplayer::Player2)
            }
        }
    };
    
    // Here we need a swapped version which make player::all don't work.
    duel.send(message.clone().into(), ExtendedNetplayer::Player(Netplayer::Player1));
    duel.send(message.swap_clone().into(), ExtendedNetplayer::Player(Netplayer::Player2));
    for index in 0..duel.observers.len() {
        if duel.observers[index].is_some() {
            duel.send(message.clone().into(), ExtendedNetplayer::Observer(index as u8));
        }
    }
    duel.send(stoc::SelectTp.into(), ExtendedNetplayer::Player(winner));
    duel.stage = DuelStage::Firstgo;
}

#[handler(ctos::TpResult)]
#[register_to(HANDLER_INFOS)]
fn on_tp_result(duel: &mut SingleDuel, player: ExtendedNetplayer, tp_result: &ctos::TpResult) {
    let tp_player = match player {
        ExtendedNetplayer::Player(n @ (Netplayer::Player1 | Netplayer::Player2)) => n,
        _ => { warn!("TpResult requested by non-player"); return; },
    };
    duel.stage = DuelStage::Dueling;
    duel.first_attack_player = if tp_result.result == CorePlayer::FirstAttackPlayer { tp_player } else {
        match tp_player { Netplayer::Player1 => Netplayer::Player2, _ => Netplayer::Player1 }
    };
    if let Some(p) = duel.players[0].as_mut() { p.state = Some(ctos::MessageType::Response); }
    if let Some(p) = duel.players[1].as_mut() { p.state = Some(ctos::MessageType::Response); }
    duel.players[0].as_mut().unwrap().time_limit = duel.host_info.time_limit;
    duel.players[1].as_mut().unwrap().time_limit = duel.host_info.time_limit;
    duel.set_player_info(CorePlayer::FirstAttackPlayer, duel.host_info.start_lp as i32, duel.host_info.start_hand as i32, duel.host_info.draw_count as i32);
    duel.set_player_info(CorePlayer::SecondAttackPlayer, duel.host_info.start_lp as i32, duel.host_info.start_hand as i32, duel.host_info.draw_count as i32);
    for &code in duel.players[0].as_ref().unwrap().deck.main.iter().rev() {
        duel.new_card(code, CorePlayer::FirstAttackPlayer, CorePlayer::FirstAttackPlayer, Location::Deck, 0, Position::FacedownDefense);
    }
    for &code in duel.players[0].as_ref().unwrap().deck.extra.iter().rev() {
        duel.new_card(code, CorePlayer::FirstAttackPlayer, CorePlayer::FirstAttackPlayer, Location::Extra, 0, Position::FacedownDefense);
    }
    for &code in duel.players[1].as_ref().unwrap().deck.main.iter().rev() {
        duel.new_card(code, CorePlayer::SecondAttackPlayer, CorePlayer::SecondAttackPlayer, Location::Deck, 0, Position::FacedownDefense);
    }
    for &code in duel.players[1].as_ref().unwrap().deck.extra.iter().rev() {
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
    duel.send(stoc::GameMessage { message: start(0) }.into(), ExtendedNetplayer::Player(Netplayer::Player1));
    duel.send(stoc::GameMessage { message: start(1) }.into(), ExtendedNetplayer::Player(Netplayer::Player2));
    let obs_message: stoc::Message = stoc::GameMessage { message: start(0x10) }.into();
    for index in 0..duel.observers.len() {
        if duel.observers[index].is_some() {
            duel.send(obs_message.clone(), ExtendedNetplayer::Observer(index as u8));
        }
    }
    let mut options = DuelOptions::empty();
    if duel.host_info.no_shuffle_deck { options.insert(DuelOptions::PseudoShuffle); }
    duel.start(options, duel.host_info.duel_rule);
    if duel.host_info.time_limit > 0 {
        let time_limit = duel.host_info.time_limit;
        duel.time_elapsed = 0;
        if let Some(player1) = duel.players[0].as_mut() {
            player1.time_compensator = time_limit;
            player1.time_backed = time_limit;
        }
        if let Some(player2) = duel.players[1].as_mut() {
            player2.time_compensator = time_limit;
            player2.time_backed = time_limit;
        }
        let sender = duel.request_sender.clone();
        duel.timer_task = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if sender.send(Request::TimerTick).is_err() { break; }
            }
        }));
    }
}

#[handler(ctos::UpdateDeck)]
#[register_to(HANDLER_INFOS)]
fn on_update_deck(duel: &mut SingleDuel, player: ExtendedNetplayer, update_deck: &ctos::UpdateDeck) {
    let netplayer = match player {
        ExtendedNetplayer::Player(n) => n,
        _ => { warn!("UpdateDeck requested by non-player"); return; }
    };
    let duel_count = duel.duel_count;
    let lflist_hash = duel.host_info.lflist as u32;
    let rule = duel.host_info.rule;
    let Some(duel_player) = duel.get_player_mut(player) else {
        warn!("UpdateDeck requested by non-player");
        return;
    };
    if duel_player.ready {
        warn!("UpdateDeck requested but player is already ready");
        return;
    }
    duel_player.deck = update_deck.deck.clone();
    let deck_manager = managers::deck_manager::load();
    let data_manager = managers::data_manager::load();
    let lflist = deck_manager.as_ref().and_then(|dm| dm.get_lflist(lflist_hash));
    let (Some(lflist), Some(data_manager)) = (lflist, data_manager.as_ref()) else { return; };
    if let Err(deck_error) = duel_player.deck.prepare(
        lflist.clone(),
        rule,
        |code| data_manager.get_card(code),
    ) {
        if duel_count == 0 {
            duel.send(stoc::HsPlayerChange { status: PlayerChange::Notready(netplayer) }.into(), player);
        }
        duel.send(stoc::ErrorMessage { err: constants::ErrorMessage::DeckError(deck_error) }.into(), player);
        return;
    }
    duel_player.ready = true;
    if duel_count == 0 {
        duel.send(stoc::HsPlayerChange { status: PlayerChange::Ready(netplayer) }.into(), ExtendedNetplayer::All);
    } else {
        duel.send(stoc::DuelStart.into(), player);
        let player1_ready = duel.players[0].as_ref().map_or(false, |p| p.ready);
        let player2_ready = duel.players[1].as_ref().map_or(false, |p| p.ready);
        if player1_ready && player2_ready {
            // TODO: determine tp_player based on match state
            let tp_netplayer = Netplayer::Player1;
            if let Some(tp) = duel.players[tp_netplayer as u8 as usize].as_mut() {
                tp.state = Some(ctos::MessageType::TpResult);
            }
            let opponent = if tp_netplayer == Netplayer::Player1 { 1 } else { 0 };
            if let Some(other) = duel.players[opponent].as_mut() {
                other.state = None;
            }
            duel.send(stoc::SelectTp.into(), ExtendedNetplayer::Player(tp_netplayer));
            duel.stage = DuelStage::Firstgo;
        }
    }
}

#[handler(ctos::CreateGame)]
#[register_to(HANDLER_INFOS)]
fn on_create_game(duel: &mut SingleDuel, create_game: &ctos::CreateGame) {
    duel.host_info = create_game.info.clone();
    duel.name = create_game.name.clone();
    duel.pass = create_game.pass.clone();
}

#[handler(ctos::JoinGame)]
#[register_to(HANDLER_INFOS)]
fn on_join_game(duel: &mut SingleDuel, join_game: &ctos::JoinGame) -> Result<Vec<stoc::Message>, stoc::Message> {
    if join_game.version != crate::PRO_VERSION {
        return Err(stoc::ErrorMessage { err: constants::ErrorMessage::VersionError(crate::PRO_VERSION) }.into());
    }
    if join_game.pass != duel.pass {
        return Err(stoc::ErrorMessage { err: constants::ErrorMessage::JoinError(JoinError::WrongPassword) }.into());
    }
    let mut response_messages = vec![];

    // calculate current user position
    let is_creator = duel.players[0].is_none() && duel.players[1].is_none() && duel.observers.is_empty();
    let pos = if duel.players[0].is_none() { Netplayer::Player1 }
                                else if duel.players[1].is_none() { Netplayer::Player2 }
                                else                              { Netplayer::Observer };
    if is_creator { duel.host_player = pos; }
    let observer_index = if pos == Netplayer::Observer {
        duel.observers.iter().position(|v| v.is_none()).unwrap_or(duel.observers.len()) as u8
    } else { 0 };
    let observer_count = duel.observer_count();

    response_messages.push(stoc::JoinGame{ info: duel.host_info.clone() }.into());
    response_messages.push(stoc::TypeChange{ change: constants::TypeChange { 
        player: pos, 
        host: is_creator, // obviously, when join game, host can only be when is creator.
        observer_index 
    }}.into());
    
    // broadcast player change
    let player = duel.last_init_player.take().expect("cannot get init player when join game");
    if pos == Netplayer::Observer {
        duel.send(stoc::HsWatchChange { watch_count: observer_count }.into(), ExtendedNetplayer::All);
    } else {
        duel.send(stoc::HsPlayerEnter { name: player.name.clone(), pos }.into(), ExtendedNetplayer::All);
    }

    // tell current user now how room is now.
    if let Some(exist_player) = duel.players[0].as_ref() {
        response_messages.push(stoc::HsPlayerEnter { name: exist_player.name.clone(), pos: Netplayer::Player1 }.into());
    }
    if let Some(exist_player) = duel.players[1].as_ref() {
        response_messages.push(stoc::HsPlayerEnter { name: exist_player.name.clone(), pos: Netplayer::Player2 }.into());
    }
    if observer_count > 0 {
        response_messages.push(stoc::HsWatchChange{ watch_count: observer_count }.into());
    }

    // actual player change
    if pos == Netplayer::Observer {
        duel.observers[observer_index as usize] = Some(player);
    } else {
        let player: DuelPlayer = player.into();
        match pos {
            Netplayer::Player1 => duel.players[0] = Some(player),
            Netplayer::Player2 => duel.players[1] = Some(player),
            _ => panic!("try to put into a illegal player pos")
        };
    }

    Ok(response_messages)
}

#[handler(ctos::HsToDuelist)]
#[register_to(HANDLER_INFOS)]
fn on_hs_to_duelist(duel: &mut SingleDuel, player: ExtendedNetplayer) -> Option<stoc::Message> {
    let observer_index = if let ExtendedNetplayer::Observer(observer_index) = player {
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
    let new_position = if duel.players[0].is_none() { Netplayer::Player1 } else { Netplayer::Player2 };
    let name = observer.name.clone();
    duel.players[new_position as u8 as usize] = Some(observer.into());
    duel.send(stoc::HsPlayerEnter { name, pos: new_position }.into(), ExtendedNetplayer::All);
    duel.send(stoc::HsWatchChange { watch_count: duel.observer_count() }.into(), ExtendedNetplayer::All);
    Some(stoc::TypeChange {
        change: TypeChange {
            player: new_position,
            host: false,
            observer_index: 0,
        }
    }.into())
}

#[handler(ctos::HsToObserver)]
#[register_to(HANDLER_INFOS)]
fn on_hs_to_observer(duel: &mut SingleDuel, player: ExtendedNetplayer) -> Option<stoc::Message> {
    let original_netplayer = match player {
        ExtendedNetplayer::Player(netplayer @ (Netplayer::Player1 | Netplayer::Player2)) => netplayer,
        _ => {
            warn!("to_observer requested by illegal player");
            return None;
        },
    };
    let position = original_netplayer as u8 as usize;
    let Some(player) = duel.players[position].take() else {
        warn!("to_observer requested but player slot is empty");
        return None;
    };
    duel.send(stoc::HsPlayerChange { 
        status: PlayerChange::Observe(original_netplayer) 
    }.into(), ExtendedNetplayer::All);
    let observer_count = duel.observer_count();
    let observer_slot = duel.observers.iter().position(|v| v.is_none()).unwrap_or(duel.observers.len());
    if observer_slot == duel.observers.len() {
        duel.observers.push(Some(player.player));
    } else {
        duel.observers[observer_slot] = Some(player.player);
    }
    duel.send(stoc::HsWatchChange { watch_count: observer_count }.into(), ExtendedNetplayer::All);
    Some(stoc::TypeChange {
        change: TypeChange {
            player: Netplayer::Observer,
            host: duel.host_player == original_netplayer,
            observer_index: observer_slot as u8,
        }
    }.into())
}

#[handler(ctos::LeaveGame)]
#[register_to(HANDLER_INFOS)]
fn on_leave_game(duel: &mut SingleDuel, player: ExtendedNetplayer) -> bool {
    if player == ExtendedNetplayer::Player(duel.host_player) {
        let new_host = if duel.players[0].is_some() && player != ExtendedNetplayer::Player(Netplayer::Player1) {
            Netplayer::Player1
        } else if duel.players[1].is_some() && player != ExtendedNetplayer::Player(Netplayer::Player2) {
            Netplayer::Player2
        } else {
            duel.end();
            return true;
        };
        duel.host_player = new_host;
        if duel.stage == DuelStage::Begin {
            let new_host_index = new_host as u8 as usize;
            duel.players[new_host_index].as_mut().unwrap().ready = false;
            duel.send(stoc::TypeChange {
                change: TypeChange {
                    player: new_host,
                    host: true,
                    observer_index: 0,
                }
            }.into(), ExtendedNetplayer::Player(new_host));
        }
    }

    match player {
        ExtendedNetplayer::Observer(observer_index) => {
            let index = observer_index as usize;
            duel.observers[index] = None;
            if duel.stage == DuelStage::Begin {
                let observer_count = duel.observers.iter().filter(|v| v.is_some()).count() as u16;
                duel.send(stoc::HsWatchChange { watch_count: observer_count }.into(), ExtendedNetplayer::All);
            }
        }
        ExtendedNetplayer::Player(leaving_netplayer) => {
            if duel.stage == DuelStage::Begin {
                duel.players[leaving_netplayer as u8 as usize] = None;
                let leave_message: stoc::Message = stoc::HsPlayerChange { status: PlayerChange::Leave(leaving_netplayer) }.into();
                duel.send(leave_message, ExtendedNetplayer::All);
            } else {
                if duel.stage == DuelStage::Siding {
                    if duel.players[0].as_ref().map_or(false, |p| !p.ready) {
                        duel.send(stoc::DuelStart.into(), ExtendedNetplayer::Player(Netplayer::Player1));
                    }
                    if duel.players[1].as_ref().map_or(false, |p| !p.ready) {
                        duel.send(stoc::DuelStart.into(), ExtendedNetplayer::Player(Netplayer::Player2));
                    }
                }
                if duel.stage != DuelStage::End {
                    let winner = duel.to_core_player(leaving_netplayer).opponent();
                    let win_message = gm::Message::Win(gm::Win {winner, reason: WinReason::OpponentLeave});
                    duel.send(stoc::GameMessage { message: win_message }.into(),ExtendedNetplayer::All);
                    duel.send(stoc::DuelEnd.into(), ExtendedNetplayer::All);
                    duel.end();
                    duel.players[leaving_netplayer as u8 as usize] = None;
                    return true;
                }
            }
            duel.players[leaving_netplayer as u8 as usize] = None;
        }
        ExtendedNetplayer::Unknown => {}
        _ => {}
    }
    false
}

#[handler(ctos::HsStart)]
#[register_to(HANDLER_INFOS)]
fn on_hs_start(duel: &mut SingleDuel, player: ExtendedNetplayer) {
    let sender = match player {
        ExtendedNetplayer::Player(n) => n,
        _ => { warn!("HsStart requested by non-player"); return; },
    };
    if sender != duel.host_player {
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

    duel.send(stoc::DuelStart.into(), ExtendedNetplayer::All);

    let player1_count = stoc::DeckCount {
        mainc_s: deck1_main, sidec_s: deck1_side, extrac_s: deck1_extra,
        mainc_o: deck2_main, sidec_o: deck2_side, extrac_o: deck2_extra,
    };
    let player2_count = stoc::DeckCount {
        mainc_s: deck2_main, sidec_s: deck2_side, extrac_s: deck2_extra,
        mainc_o: deck1_main, sidec_o: deck1_side, extrac_o: deck1_extra,
    };
    duel.send(player1_count.into(), ExtendedNetplayer::Player(Netplayer::Player1));
    duel.send(player2_count.into(), ExtendedNetplayer::Player(Netplayer::Player2));

    duel.send(stoc::SelectHand.into(), ExtendedNetplayer::Player(Netplayer::Player1));
    duel.send(stoc::SelectHand.into(), ExtendedNetplayer::Player(Netplayer::Player2));

    let (player1, player2) = duel.players.split_at_mut(1);
    let player1 = player1[0].as_mut().unwrap();
    let player2 = player2[0].as_mut().unwrap();
    player1.hand_result = None;
    player2.hand_result = None;
    player1.state = Some(ctos::MessageType::HandResult);
    player2.state = Some(ctos::MessageType::HandResult);
    duel.stage = DuelStage::Finger;
}

#[handler(ctos::Surrender)]
#[register_to(HANDLER_INFOS)]
fn on_surrender(duel: &mut SingleDuel, player: ExtendedNetplayer) {
    let surrendering_player = match player {
        ExtendedNetplayer::Player(n @ (Netplayer::Player1 | Netplayer::Player2)) => n,
        _ => { warn!("Surrender requested by non-player"); return; },
    };
    if duel.stage != DuelStage::Dueling {
        warn!("Surrender requested but not in dueling stage");
        return;
    }
    let core_surrendering = duel.to_core_player(surrendering_player);
    duel.win_and_end(core_surrendering, WinReason::OpponentSurrender);
}

#[handler(ctos::TimeConfirm)]
#[register_to(HANDLER_INFOS)]
fn on_time_confirm(duel: &mut SingleDuel, player: ExtendedNetplayer) {
    if duel.host_info.time_limit == 0 { return; }
    let confirming_player = match player {
        ExtendedNetplayer::Player(n) => n,
        _ => { warn!("TimeConfirm requested by non-player"); return; },
    };
    if confirming_player != duel.last_response {
        warn!("TimeConfirm requested by wrong player");
        return;
    }
    let Some(duel_player) = duel.players[confirming_player as usize].as_mut() else {
        warn!("TimeConfirm requested but player slot is empty");
        return;
    };
    duel_player.state = Some(ctos::MessageType::Response);
    if duel.time_elapsed < 10 && duel.time_elapsed <= duel_player.time_compensator {
        duel_player.time_compensator -= duel.time_elapsed;
    } else {
        duel_player.time_limit = duel_player.time_limit.saturating_sub(duel.time_elapsed);
    }
    duel.time_elapsed = 0;
}

#[handler(ctos::Chat)]
#[register_to(HANDLER_INFOS)]
fn on_chat(duel: &mut SingleDuel, player: ExtendedNetplayer, chat: &ctos::Chat) {
    let chat = stoc::Chat {
        player: player.into(),
        msg: chat.msg.clone()
    };
    duel.send(chat.into(), ExtendedNetplayer::All);
}

#[handler(ctos::PlayerInfo)]
#[register_to(HANDLER_INFOS)]
fn on_player_info(duel: &mut SingleDuel, player_info: &ctos::PlayerInfo) {
    if let Some(player) = duel.last_init_player.as_mut() {
        player.name = player_info.name.clone();
    } else {
        warn!("We receive a player_info, but no user is waiiting init.");
    }
}

#[handler(ctos::HsReady)]
#[register_to(HANDLER_INFOS)]
fn on_hs_ready(duel: &mut SingleDuel, player: ExtendedNetplayer) -> Vec<stoc::Message> {
    let netplayer = match player {
        ExtendedNetplayer::Player(n) => n,
        _ => { warn!("HsReady requested by non-player"); return vec![]; }
    };
    if duel.stage != DuelStage::Begin {
        warn!("HsReady requested outside Begin stage");
        return vec![];
    }
    let no_check_deck = duel.host_info.no_check_deck;
    let lflist_hash = duel.host_info.lflist as u32;
    let rule = duel.host_info.rule;
    let Some(duel_player) = duel.get_player_mut(player) else {
        warn!("HsReady requested by non-player");
        return vec![];
    };
    if duel_player.ready {
        warn!("HsReady requested but player is already ready");
        return vec![];
    }
    if !no_check_deck {
        let deck_manager = managers::deck_manager::load();
        let data_manager = managers::data_manager::load();
        let lflist = deck_manager.as_ref().and_then(|dm| dm.get_lflist(lflist_hash));
        let (Some(lflist), Some(data_manager)) = (lflist, data_manager.as_ref()) else { return vec![]; };
        if let Err(deck_error) = duel_player.deck.prepare(
            lflist.clone(),
            rule,
            |code| data_manager.get_card(code)
        ) {
            duel.send(stoc::HsPlayerChange { status: PlayerChange::Notready(netplayer) }.into(), player);
            duel.send(stoc::ErrorMessage { err: constants::ErrorMessage::DeckError(deck_error) }.into(), player);
            return vec![];
        }
    }
    duel_player.ready = true;
    duel.send(stoc::HsPlayerChange {
        status: PlayerChange::Ready(netplayer)
    }.into(), ExtendedNetplayer::All);
    vec![]
}

#[handler(ctos::HsNotReady)]
#[register_to(HANDLER_INFOS)]
fn on_hs_not_ready(duel: &mut SingleDuel, player: ExtendedNetplayer) {
    if duel.stage != DuelStage::Begin { 
        warn!("HsNotReady requested outside Begin stage"); 
        return; 
    }
    let Some(duel_player) = duel.get_player_mut(player) else {
        warn!("HsNotReady requested by non-player");
        return;
    };
    if !duel_player.ready { 
        warn!("HsNotReady requested but player is already not ready"); 
        return 
    }
    duel_player.ready = false;
    duel.send(stoc::HsPlayerChange { 
        status: PlayerChange::Notready(player.into()) 
    }.into(), ExtendedNetplayer::All);
}

#[handler(ctos::HsKick)]
#[register_to(HANDLER_INFOS)]
fn on_hs_kick() -> &'static str {
    "kick"
}

#[handler(ctos::RequestField)]
#[register_to(HANDLER_INFOS)]
fn on_request_field(duel: &mut SingleDuel) {
    todo!()
}

// ============== Game Message Dispatch ==============

/// `m.selecting_player` / `m.player` fields are `CorePlayer` (engine player id).
/// They are translated to `Player` (array index) via `engine_to_player()` before
/// being passed to helper functions.
fn dispatch_game_message(duel: &mut SingleDuel, msg: &gm::Message) {
    // match msg {
    //     gm::Message::Waiting(_)
    //     | gm::Message::NewTurn(_)
    //     | gm::Message::NewPhase(_)
    //     | gm::Message::ShuffleDeck(_)
    //     | gm::Message::RefreshDeck(_)
    //     | gm::Message::SwapGraveDeck(_)
    //     | gm::Message::ReverseDeck(_)
    //     | gm::Message::DeckTop(_)
    //     | gm::Message::Summoned(_)
    //     | gm::Message::Spsummoned(_)
    //     | gm::Message::Flipsummoned(_)
    //     | gm::Message::Chaining(_)
    //     | gm::Message::Chained(_)
    //     | gm::Message::ChainSolving(_)
    //     | gm::Message::ChainSolved(_)
    //     | gm::Message::ChainEnd(_)
    //     | gm::Message::ChainNegated(_)
    //     | gm::Message::ChainDisabled(_)
    //     | gm::Message::Damage(_)
    //     | gm::Message::Recover(_)
    //     | gm::Message::BecomeTarget(_)
    //     | gm::Message::ConfirmDecktop(_)
    //     | gm::Message::ConfirmExtraTop(_)
    //     | gm::Message::RandomSelected(_)
    //     | gm::Message::FieldDisabled(_)
    //     | gm::Message::Attack(_)
    //     | gm::Message::Battle(_)
    //     | gm::Message::AttackDisabled(_)
    //     | gm::Message::DamageStepStart(_)
    //     | gm::Message::DamageStepEnd(_)
    //     | gm::Message::Equip(_)
    //     | gm::Message::Unequip(_)
    //     | gm::Message::Lpupdate(_)
    //     | gm::Message::CardTarget(_)
    //     | gm::Message::CancelTarget(_)
    //     | gm::Message::PayLpcost(_)
    //     | gm::Message::AddCounter(_)
    //     | gm::Message::RemoveCounter(_)
    //     | gm::Message::MissedEffect(_)
    //     | gm::Message::BeChainTarget(_)
    //     | gm::Message::CreateRelation(_)
    //     | gm::Message::ReleaseRelation(_)
    //     | gm::Message::TossCoin(_)
    //     | gm::Message::TossDice(_)
    //     | gm::Message::RockPaperScissors(_)
    //     | gm::Message::AnnounceRace(_)
    //     | gm::Message::AnnounceAttribute(_)
    //     | gm::Message::AnnounceCard(_)
    //     | gm::Message::AnnounceNumber(_)
    //     | gm::Message::CardHint(_)
    //     | gm::Message::TagSwap(_)
    //     | gm::Message::ReloadField(_)
    //     | gm::Message::AIName(_)
    //     | gm::Message::ShowHint(_)
    //     | gm::Message::PlayerHint(_)
    //     | gm::Message::MatchKill(_)
    //     | gm::Message::CustomMsg(_) => {
    //         broadcast_message_to_all(single_duel, msg);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::Summoning(_)
    //     | gm::Message::Flipsummoning(_)
    //     | gm::Message::Set(_)
    //     | gm::Message::Swap(_)
    //     | gm::Message::PositionChange(_) => {
    //         broadcast_with_masked_card_to_inactive(single_duel, msg);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::Move(_) => {
    //         send_move_with_masked_card_to_opponent(single_duel, msg);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::ConfirmCards(m) => {
    //         // `m.player` is engine-coordinate CorePlayer; translate to array-slot Player.
    //         let player = single_duel.engine_to_player(m.player);
    //         send_confirm_cards_with_deck_masked(single_duel, msg, player);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::ShuffleHand(m) => {
    //         // `m.player` is engine-coordinate CorePlayer; translate to array-slot Player.
    //         let player = single_duel.engine_to_player(m.player);
    //         send_shuffle_hand_active_full_opponent_masked(single_duel, msg, player);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::ShuffleExtra(m) => {
    //         // `m.player` is engine-coordinate CorePlayer; translate to array-slot Player.
    //         let player = single_duel.engine_to_player(m.player);
    //         send_shuffle_extra_active_full_opponent_masked(single_duel, msg, player);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::Draw(m) => {
    //         // `m.player` is engine-coordinate CorePlayer; translate to array-slot Player.
    //         let player = single_duel.engine_to_player(m.player);
    //         send_draw_active_full_opponent_masked(single_duel, msg, player);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::SpecialSummoning(_) => {
    //         send_spsummoning_active_full_opponent_masked(single_duel, msg);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::Win(m) => on_win(single_duel, m),

    //     gm::Message::Retry(_) => {
    //         resend_retry_to_player(single_duel, msg);
    //         DispatchResult::WaitForResponse
    //     }

    //     gm::Message::SelectBattleCommand(m) => {
    //         // `m.selecting_player` is engine-coordinate CorePlayer; translate to array-slot Player.
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectIdleCommand(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectEffectYesNo(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectYesNo(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);d
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectOption(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectChain(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectPlace(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectPosition(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectCounter(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectSum(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectDisableField(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SortCard(m) => {
    //         // `m.player` is engine-coordinate CorePlayer; translate to array-slot Player.
    //         let player = single_duel.engine_to_player(m.player);
    //         send_to_player_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }

    //     gm::Message::SelectCard(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_select_cards_masked_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectTribute(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_select_cards_masked_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }
    //     gm::Message::SelectUnselectCard(m) => {
    //         let player = single_duel.engine_to_player(m.selecting_player);
    //         send_select_cards_masked_and_wait(single_duel, msg, player);
    //         DispatchResult::WaitForResponse
    //     }

    //     gm::Message::Hint(_) => {
    //         dispatch_hint_by_type(single_duel, msg);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::HandResult(_) => {
    //         broadcast_message_to_all(single_duel, msg);
    //         DispatchResult::Continue
    //     }

    //     gm::Message::CardSelected(_) => DispatchResult::Continue,

    //     gm::Message::Start(_) => {
    //         todo!("broadcast_game_start_to_all_with_perspective_info")
    //     }
    //     gm::Message::UpdateData(_) => {
    //         todo!("send_update_data_to_all_clients")
    //     }
    //     gm::Message::UpdateCard(_) => {
    //         todo!("send_update_card_to_all_clients")
    //     }
    //     gm::Message::RequestDeck(_) => {
    //         todo!("request_deck_from_player_and_wait")
    //     }
    //     gm::Message::SortChain(_) => {
    //         todo!("send_sort_chain_to_player_and_wait")
    //     }
    //     gm::Message::ShuffleSetCard(_) => {
    //         todo!("shuffle_set_card_and_broadcast_to_all")
        // }
    // }
}

// ============== Game Message Dispatch Helpers ==============

fn broadcast_message_to_all(duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

fn broadcast_with_masked_card_to_inactive(duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_to_player_and_wait(duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

/// `selecting_player`: array index (already translated from engine coordinates).
fn send_select_cards_masked_and_wait(duel: &mut SingleDuel, msg: &gm::Message, selecting_player: Netplayer) {
    todo!()
}

fn resend_retry_to_player(duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_shuffle_hand_active_full_opponent_masked(duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_shuffle_extra_active_full_opponent_masked(duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_draw_active_full_opponent_masked(duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

fn send_spsummoning_active_full_opponent_masked(duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

fn send_move_with_masked_card_to_opponent(duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_confirm_cards_with_deck_masked(duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

fn dispatch_hint_by_type(duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

// fn on_win(duel: &mut SingleDuel, win: &gm::Win) -> DispatchResult {
//     todo!()
// }

// ============== Refresh Helpers ==============
// The `player` parameter is an array index; translated to engine coordinates inside.


fn refresh_mzone(duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_szone(duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_hand(duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_extra(duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_grave(duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_removed(duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}

fn refresh_all(duel: &mut SingleDuel) {
    todo!()
}


fn refresh_single(duel: &mut SingleDuel, player: Netplayer, location: i32, sequence: i32) {
    todo!()
}

fn tick(duel: &mut SingleDuel) {
    todo!()
}

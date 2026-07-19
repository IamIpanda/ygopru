// 对应: ../ygopro/gframe/single_duel.h, ../ygopro/gframe/single_duel.cpp
// YGOPRO_SERVER_MODE

use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::OnceLock;

use arc_swap::ArcSwap;
use futures::Stream;
use linkme::distributed_slice;
use parking_lot::ArcMutexGuard;
use parking_lot::RawMutex;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use ygopro_core_wrapper::DuelSeed;
use ygopro_core_wrapper as core;
use ygopro_data::constants::*;
use ygopro_data::data::DeckCheckError;
use ygopro_data::data::Deck;
use ygopro_data::message::game_message as gm;
use ygopro_data::message::ctos;
use ygopro_data::message::stoc;
use ygopro_handler::Processor;
use ygopro_handler::RoomProvider;
use ygopro_data::string::FixedLengthString;

use crate::common;

type Handler = common::Handler<SingleDuel>;
#[distributed_slice]
pub static HANDLERS: [Handler];
static YGOPRO_PROCESSOR: OnceLock<ArcSwap<Processor<u8, ctos::Message, common::State<SingleDuel>, common::Response, Handler>>> = OnceLock::new();

pub fn reset_processor() -> &'static ArcSwap<Processor<u8, ctos::Message, common::State<SingleDuel>, common::Response, Handler>> {
    YGOPRO_PROCESSOR.get_or_init(|| {
        let mut processor = Processor::new();
        for handler in HANDLERS.iter() {
            processor.register_global(handler.clone());
        }
        processor.resolve();
        ArcSwap::from(Arc::new(processor))
    })
}


pub struct DuelPlayer {
    player: common::DuelPlayer,
    ready: bool,
    deck: Deck,
    deck_error: Option<DeckCheckError>,
    hand_result: Hand,
    time_limit: u16,
    time_compensator: u16,
    time_backed: u16,
    stoc_sender: mpsc::UnboundedSender<stoc::Message>,
}

impl DuelPlayer {
    fn new(stoc_sender: mpsc::UnboundedSender<stoc::Message>) -> Self {
        Self {
            player: common::DuelPlayer {
                name: FixedLengthString::allocate(),
                state: None,
            },
            ready: false,
            deck: Deck::new(),
            deck_error: None,
            hand_result: Hand::Scissors,
            time_limit: 0,
            time_compensator: 0,
            time_backed: 0,
            stoc_sender,
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

pub struct SingleDuel {
    duel: common::DuelMode,
    players: [Option<DuelPlayer>; 2], // indexed by NetPlayer
    first_attack_player: Netplayer,
    last_response: Netplayer,
    turn_player: Netplayer,
    phase: Phase,
    deck_reversed: bool,
    is_match: bool,
    match_kill_card_code: i32,
    duel_count: u8,
    match_winner: Vec<Netplayer>,
    time_elapsed: u16,
    messages: Vec<stoc::Message>,
    observers: Vec<common::DuelPlayer>,
    observer_messages: Vec<stoc::Message>,
    observer_sender: broadcast::Sender<stoc::Message>,
    observer_receiver: broadcast::Receiver<stoc::Message>
}
// IN -> (NetPlayer, CTOS::Message)
// Out -> Vec<(NetPlayer, Wrappeed<STOC::Message>)>

impl Deref for SingleDuel {
    type Target = common::DuelMode;
    fn deref(&self) -> &Self::Target { &self.duel }
}

impl DerefMut for SingleDuel {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.duel }
}

impl SingleDuel {
    pub fn new(is_match: bool, seed: DuelSeed) -> Self {
        let (observer_sender, observer_receiver) = broadcast::channel(128);
        Self {
            duel: common::DuelMode {
                host_player: Netplayer::Player1,
                host_info: Default::default(),
                duel_stage: DuelStage::Begin,
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
            observer_receiver
        }
    }

    fn send(&mut self, message: stoc::Message, target: Netplayer) {
        let masked_message = {
            let mut m = message.clone();
            mask_stoc_message(&mut m);
            m
        };
        self.messages.push(message.clone());
        self.observer_messages.push(masked_message.clone());
        self.observer_sender.send(masked_message.clone()).ok();


    }

    pub fn subscribe_observer(&self) -> impl Stream<Item = stoc::Message> {
        let history = tokio_stream::iter(self.observer_messages.clone());
        let live = BroadcastStream::new(self.observer_sender.subscribe())
            .filter_map(|result| result.ok());
        history.chain(live)
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
}


impl RoomProvider<ctos::Message, stoc::Message> for SingleDuel {
    type ServerToClientStream = UnboundedReceiverStream<stoc::Message>;

    fn add(&mut self, client_to_server_stream: impl Stream<Item = ctos::Message> + Unpin + Send + 'static) -> Self::ServerToClientStream {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut is_observer = true;
        if let Some(player_index) = self.players.iter().position(|p| p.is_none()) {
            self.players[player_index] = Some(DuelPlayer::new(sender));
            is_observer = false;
        } else {
            let mut observer_stream = self.subscribe_observer();
            let bridge_sender = sender;
            tokio::spawn(async move {
                while let Some(message) = observer_stream.next().await {
                    if bridge_sender.send(message).is_err() {
                        break;
                    }
                }
            });
        }

        tokio::spawn(async move {
            let mut stream = Box::pin(client_to_server_stream);
            while let Some(message) = stream.next().await {
                if (is_observer) {
                    todo!()
                }
                // process ctos message, produce stoc responses via sender
            }
        });

        UnboundedReceiverStream::new(receiver)
    }
}

fn mask_stoc_message(message: &mut stoc::Message) {
    if let stoc::Message::GameMessage(g) = message {
        mask_game_message(&mut g.message);
    }
}

fn mask_game_message(message: &mut gm::Message) {
    match message {
        gm::Message::Move(m) => {
            let hide = !m.current.0.location.intersects(Location::Grave | Location::Overlay)
                && (m.current.0.location.intersects(Location::Deck | Location::Hand) || m.current.1.is_face_down());
            if hide {
                m.code = 0;
            }
        }
        gm::Message::Set(m) => {
            m.position.0.code = 0;
        }
        gm::Message::SpecialSummoning(m) => {
            if m.position.1.is_face_down() {
                m.position.0.code = 0;
            }
        }
        gm::Message::Draw(m) => {
            for code in &mut m.codes {
                if (*code >> 24) & 0x80 == 0 {
                    *code = 0;
                }
            }
        }
        gm::Message::ShuffleHand(m) => {
            m.codes.fill(0);
        }
        gm::Message::ShuffleExtra(m) => {
            m.codes.fill(0);
        }
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DispatchResult {
    Continue,
    WaitForResponse,
    End,
}

fn process(single_duel: &mut SingleDuel) {
    todo!()
}

fn analyze(single_duel: &mut SingleDuel, engine_buffer: &[u8]) -> DispatchResult {
    todo!()
}

fn duel_end_proc(single_duel: &mut SingleDuel) {
    todo!()
}

fn end_duel(single_duel: &mut SingleDuel) {
    todo!()
}

/// Record which player (array index) the engine is waiting on.
fn wait_for_response(single_duel: &mut SingleDuel, player: Netplayer) {
    single_duel.last_response = player;
}

// ============== CTOS Handlers ==============

fn on_response(single_duel: &mut SingleDuel, response: &ctos::Response) {
    todo!()
}

fn on_update_deck(single_duel: &mut SingleDuel, update_deck: &ctos::UpdateDeck) {
    todo!()
}

fn on_hand_result(single_duel: &mut SingleDuel, hand_result: &ctos::HandResult) {
    todo!()
}

fn on_tp_result(single_duel: &mut SingleDuel, tp_result: &ctos::TpResult) {
    todo!()
}

fn on_player_info(duel: &mut SingleDuel, player_pos: Netplayer, player_info: &ctos::PlayerInfo) {
    match duel.players[player_pos as u8 as usize] {
        Some(ref mut p) => {
            p.name = player_info.name.clone();
        }
        None => {}
    }
}

fn on_create_game(duel: &mut SingleDuel, create_game: &ctos::CreateGame) {
    duel.host_info = create_game.info.clone();
    duel.name = create_game.name.clone();
    duel.pass = create_game.pass.clone();
}

/// `player_pos`: array index of the joining player.
fn on_join_game(duel: &mut SingleDuel, player_pos: Netplayer, join_game: &ctos::JoinGame) -> Result<(), stoc::Message> {
    // let player = match duel.players.get(player_pos as usize) {
    //     Some(Some(p)) => unsafe { &*(p as *const DuelPlayer) }, // Fuck SB rust borrow checker
    //     _ => return Err(stoc::ErrorMessage { msg: ErrorMessage::JoinError, code: 0u32 }.into()),
    // };

    // // ygopro have a reenter check here.
    // // It is not needed here because the room provider limit that will not happen.

    // if join_game.version != crate::PRO_VERSION {
    //     return Err(stoc::ErrorMessage { msg: ErrorMessage::VersionError, code: crate::PRO_VERSION as u32 }.into());
    // }

    // if join_game.pass != duel.pass {
    //     return Err(stoc::ErrorMessage { msg: ErrorMessage::JoinError, code: 1u32 }.into());
    // }

    // let target_position = if duel.players[0].is_none() { Netplayer::Player1 }
    //                             else if duel.players[1].is_none() { Netplayer::Player2 }
    //                             else                              { Netplayer::Observer };

    // if duel.players[0].is_none() && duel.players[1].is_none() && duel.observers == [] {
    //     duel.host_player = Netplayer::Player1; // As first player, it is always player 1.
    // }

    // let join_game = stoc::JoinGame { info: duel.host_info.clone() };
    // let mut type_change = stoc::TypeChange { _type: if duel.host_player as u8 == player_pos as u8  { 0x10 } else { 0 } };

    // // TODO: 这里的广播是不对的，应该去掉自己。
    // if matches!(target_position, Netplayer::Observer) {
    //     duel.observers.push(());
    //     type_change._type |= Netplayer::Observer as u8;
    //     let watch_change = stoc::HsWatchChange { watch_count: duel.observers.len() as u16 };
    //     duel.send(watch_change.into(), Player::Both, Player::None);
    // }
    // else {
    //     let player_enter = stoc::HsPlayerEnter { name: Netplayer.name.clone(), pos: target_position };
    //     duel.send(player_enter.into(), Player::Both, Player::None);
    //     type_change._type |= target_position as u8;
    // }

    // duel.send(join_game.into(), Player::Both, Player::None);
    // duel.send(type_change.into(), Player::Both, Player::None);

    // for i in 0..2 {
    //     if i == player_pos as usize { continue; }
    //     let (name, ready, pos) = match &duel.players[i] {
    //         Some(p) => (p.name.clone(), p.ready, Netplayer::try_from(i as u8).unwrap_or(Netplayer::Observer)),
    //         None => continue,
    //     };
    //     duel.send(stoc::HsPlayerEnter { name, pos }.into(), player_pos, Player::None);
    //     if ready {
    //         let status = if i == 0 { PlayerChange::Ready(Netplayer::Player1) } else { PlayerChange::Ready(Netplayer::Player2) };
    //         duel.send(stoc::HsPlayerChange { status }.into(), player_pos, Player::None);
    //     }
    // }
    // if !duel.observers.is_empty() {
    //     duel.send(stoc::HsWatchChange { watch_count: duel.observers.len() as u16 }.into(), player_pos, Player::None);
    // }

    Ok(())

}

/// `player`: array index of the leaving player.
fn on_leave_game(single_duel: &mut SingleDuel, player: Netplayer) {
    
}

/// `player`: array index of the surrendering player.
fn on_surrender(single_duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}

fn on_time_confirm(single_duel: &mut SingleDuel) {
    todo!()
}

/// `player`: array index of the chatting player.
fn on_chat(single_duel: &mut SingleDuel, player: Netplayer, chat: &ctos::Chat) {
    let chat = stoc::Chat {
        player: todo!(),
        msg: chat.msg.clone(),
    };
}

fn on_hs_to_duelist(single_duel: &mut SingleDuel) {
    todo!()
}

fn on_hs_to_observer(single_duel: &mut SingleDuel) {
    todo!()
}

fn on_hs_ready(single_duel: &mut SingleDuel) {
    todo!()
}

fn on_hs_not_ready(single_duel: &mut SingleDuel) {
    todo!()
}

fn on_hs_kick(single_duel: &mut SingleDuel, hs_kick: &ctos::HsKick) {
    todo!()
}

fn on_hs_start(single_duel: &mut SingleDuel) {
    todo!()
}

fn on_request_field(single_duel: &mut SingleDuel) {
    todo!()
}

// ============== Game Message Dispatch ==============

/// `m.selecting_player` / `m.player` fields are `CorePlayer` (engine player id).
/// They are translated to `Player` (array index) via `engine_to_player()` before
/// being passed to helper functions.
fn dispatch_game_message(single_duel: &mut SingleDuel, msg: &gm::Message) {
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
    //         let player = single_duel.engine_to_player(m.selecting_player);
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

fn broadcast_message_to_all(single_duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

fn broadcast_with_masked_card_to_inactive(single_duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_to_player_and_wait(single_duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

/// `selecting_player`: array index (already translated from engine coordinates).
fn send_select_cards_masked_and_wait(single_duel: &mut SingleDuel, msg: &gm::Message, selecting_player: Netplayer) {
    todo!()
}

fn resend_retry_to_player(single_duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_shuffle_hand_active_full_opponent_masked(single_duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_shuffle_extra_active_full_opponent_masked(single_duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_draw_active_full_opponent_masked(single_duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

fn send_spsummoning_active_full_opponent_masked(single_duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

fn send_move_with_masked_card_to_opponent(single_duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

/// `player`: array index (already translated from engine coordinates).
fn send_confirm_cards_with_deck_masked(single_duel: &mut SingleDuel, msg: &gm::Message, player: Netplayer) {
    todo!()
}

fn dispatch_hint_by_type(single_duel: &mut SingleDuel, msg: &gm::Message) {
    todo!()
}

fn on_win(single_duel: &mut SingleDuel, win: &gm::Win) -> DispatchResult {
    todo!()
}

// ============== Refresh Helpers ==============
// The `player` parameter is an array index; translated to engine coordinates inside.


fn refresh_mzone(single_duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_szone(single_duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_hand(single_duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_extra(single_duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_grave(single_duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}


fn refresh_removed(single_duel: &mut SingleDuel, player: Netplayer) {
    todo!()
}

fn refresh_all(single_duel: &mut SingleDuel) {
    todo!()
}


fn refresh_single(single_duel: &mut SingleDuel, player: Netplayer, location: i32, sequence: i32) {
    todo!()
}

fn tick(single_duel: &mut SingleDuel) {
    todo!()
}

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]

use binrw::binrw;
use binrw::helpers::until_eof;
use ygopro_derive::Message;

use crate::constants::*;
use crate::data::CardPosition;
use crate::data::UpdateCardInfo;
use crate::utils::string::U16String;

include!(concat!(env!("OUT_DIR"), "/game_message.rs"));
every_message!(crate::generate_enum);

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 1)]
#[repr(C)] 
pub struct Retry;

#[derive(Debug, Message, Clone)]
#[message(gm, flag = 2)]
#[binrw]
#[repr(C)]
pub struct Hint {
    pub _type: crate::constants::Hint,
    pub player: Netplayer,
    pub data: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 3)]
#[repr(C)]
pub struct Waiting;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 4)]
#[repr(C)]
pub struct Start {
    pub plyaer_type: u8,
    pub rule: i8,
    pub player1_lp: i32,
    pub player2_lp: i32,
    pub player1_deck_count: u16,
    pub player1_extra_count: u16,
    pub player2_deck_count: u16,
    pub player2_extra_count: u16
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 5)]
#[repr(C)]
pub struct Win {
    pub winner: Netplayer,
    pub reason: u8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 6)]
#[repr(C)]
pub struct UpdateData {
    pub player: Netplayer,
    pub location: Location,
    #[br(parse_with=until_eof)]
    pub data: Vec<UpdateCardInfo>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 7)]
#[repr(C)]
pub struct UpdateCard {
    pub position: CardPosition<false, false, false>,
    pub data: UpdateCardInfo
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 8)]
#[repr(C)]
pub struct RequestDeck;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 10)]
#[repr(C)]
pub struct SelectBattleCommand {
    pub selecting_player: Netplayer,
    #[bw(calc(activatable_cards.len() as u8))]
    activatable_cards_size: u8,
    #[br(count = activatable_cards_size)]
    pub activatable_cards: Vec<CardPosition<true, false, true>>,
    #[bw(calc(attackable_cards.len() as u8))]
    attackable_cards_size: u8,
    #[br(count = attackable_cards_size)]
    pub attackable_cards: Vec<(CardPosition<true, false, false>, i8)>, // Diratt
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub can_enter_m2: bool, // u8
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub can_enter_ep: bool  // u8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 11)]
#[repr(C)]
pub struct SelectIdleCommand {
    pub selecting_player: Netplayer,
    #[bw(calc(summonable_cards.len() as u8))]
    summonable_cards_size: u8,
    #[br(count = summonable_cards_size)]
    pub summonable_cards: Vec<CardPosition<true, false, false>>,
    #[bw(calc(special_summonable_cards.len() as u8))]
    special_summonable_cards_size: u8,
    #[br(count = special_summonable_cards_size)]
    pub special_summonable_cards: Vec<CardPosition<true, false, false>>,
    #[bw(calc(reposable_cards.len() as u8))]
    reposable_cards_size: u8,
    #[br(count = reposable_cards_size)]
    pub reposable_cards: Vec<CardPosition<true, false, false>>,
    #[bw(calc(m_setable_cards.len() as u8))]
    m_setable_cards_size: u8,
    #[br(count = m_setable_cards_size)]
    pub m_setable_cards: Vec<CardPosition<true, false, false>>,
    #[bw(calc(s_setable_cards.len() as u8))]
    s_setable_cards_size: u8,
    #[br(count = s_setable_cards_size)]
    pub s_setable_cards: Vec<CardPosition<true, false, false>>,
    #[bw(calc(activatable_cards.len() as u8))]
    activatable_cards_size: u8,
    #[br(count = activatable_cards_size)]
    pub activatable_cards: Vec<CardPosition<true, false, true>>,
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub can_enter_bp: bool, // u8
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub can_enter_ep: bool, // u8
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub can_shuffle_hand: bool // u8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 12)]
#[repr(C)]
pub struct SelectEffectYesNo {
    pub selecting_player: Netplayer,
    pub card_position: CardPosition<true, true, true>,
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 13)]
#[repr(C)]
pub struct SelectYesNo {
    pub selecting_player: Netplayer,
    pub description: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 14)]
#[repr(C)]
pub struct SelectOption {
    pub selecting_player: Netplayer,
    #[bw(calc(options.len() as u8))]
    options_size: u8,
    #[br(count = options_size)]
    pub options: Vec<i32>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 15)]
#[repr(C)]
pub struct SelectCard {
    pub selecting_player: Netplayer,
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub select_cancelable: bool,
    pub select_min: i8,
    pub select_max: i8,
    #[bw(calc(positions.len() as u8))]
    positions_size: u8,
    #[br(count = positions_size)]
    pub positions: Vec<CardPosition<true, true, false>>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 16)]
#[repr(C)]
pub struct SelectChain {
    pub selecting_player: Netplayer,
    #[bw(calc(activatable_cards.len() as u8))]
    pub activatable_cards_count: u8,
    pub specount: u8,
    pub forced: u8,
    pub hint0: i32,
    pub hint1: i32,
    #[br(count = activatable_cards_count)]
    pub activatable_cards: Vec<(i8, CardPosition<true, true, true>)>,
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 18)]
#[repr(C)]
pub struct SelectPlace {
    pub selecting_player: Netplayer,
    pub count: i8,
    pub selectzble_field: i32,
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 19)]
#[repr(C)]
pub struct SelectPosition {
    pub selecting_player: Netplayer,
    pub code: u32,
    pub positions: Position
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 20)]
#[repr(C)]
pub struct SelectTribute {
    pub selecting_player: Netplayer,
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub cancelable: bool,
    pub select_min: i8,
    pub select_max: i8,
    #[bw(calc(tributes.len() as u8))]
    tributes_size: u8,
    #[br(count = tributes_size)]
    pub tributes: Vec<(CardPosition<true, false, false>, i8)> // Tribute
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 21)]
#[repr(C)]
pub struct SortChain;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 22)]
#[repr(C)]
pub struct SelectCounter {
    pub selecting_player: Netplayer,
    pub select_counter_type: i16,
    pub select_counter_count: i16,
    #[bw(calc(selectable_cards.len() as u8))]
    selectable_cards_size: u8,
    #[br(count = selectable_cards_size)]
    pub selectable_cards: Vec<(CardPosition<true, true, false>, i8)> // OpParam
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 23)]
#[repr(C)]
pub struct SelectSum {
    pub select_mode: i8,
    pub selecting_player: i8, // Player
    pub select_sum_value: i32,
    pub select_min: i8,
    pub select_max: i8,
    #[bw(calc(must_select_cards.len() as u8))]
    must_select_cards_size: u8,
    #[br(count = must_select_cards_size)]
    pub must_select_cards: Vec<(CardPosition<true, false, false>, i32)>, // OpParam
    #[bw(calc(select_cards.len() as u8))]
    select_cards_size: u8,
    #[br(count = select_cards_size)]
    pub select_cards: Vec<(CardPosition<true, false, false>, i32)> // OpParam
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 24)]
#[repr(C)]
pub struct SelectDisableField {
    pub selecting_player: Netplayer,
    pub count: i8,
    pub selectzble_field: i32,
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 25)]
#[repr(C)]
pub struct SortCard {
    pub player: Netplayer,
    #[bw(calc(cards.len() as u8))]
    cards_size: u8,
    #[br(count = cards_size)]
    pub cards: Vec<CardPosition<true, false, false>>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 26)]
#[repr(C)]
pub struct SelectUnselectCard {
    pub selecting_playuer: Netplayer,
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub able: bool,
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub cancelable: bool,
    pub select_min: i8,
    pub select_max: i8,
    #[bw(calc(positions1.len() as u8))]
    positions1_size: u8,
    #[br(count = positions1_size)]
    pub positions1: Vec<CardPosition<true, true, false>>,
    #[bw(calc(positions2.len() as u8))]
    positions2_size: u8,
    #[br(count = positions2_size)]
    pub positions2: Vec<CardPosition<true, true, false>>
}

#[binrw]
#[derive(Clone, Debug, Message)]
#[message(gm, flag = 30)]
pub struct ConfirmDecktop {
    pub controller: LocalPlayer,
    #[bw(calc(codes.len() as u8))]
    codes_size: u8,
    #[br(count = codes_size)]
    pub codes: Vec<i32>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 31)]
#[repr(C)]
pub struct ConfirmCards {
    pub player: LocalPlayer,
    #[bw(calc(cards.len() as u8))]
    cards_size: u8,
    #[br(count = cards_size)]
    pub cards: Vec<CardPosition<true, false, false>>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 32)]
#[repr(C)]
pub struct ShuffleDeck {
    pub player: LocalPlayer 
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 33)]
#[repr(C)]
pub struct ShuffleHand {
    pub player: LocalPlayer,
    pub count: u8,
    #[br(parse_with=until_eof)]
    pub codes: Vec<u32>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 34)]
#[repr(C)]
pub struct RefreshDeck {
    pub player: LocalPlayer
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 35)]
#[repr(C)]
pub struct SwapGraveDeck {
    pub player: LocalPlayer 
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 36)]
#[repr(C)]
pub struct ShuffleSetCard {
    pub location: Location,
    #[bw(calc(mc.len() as u8))]
    mc_size: u8,
    #[br(count = mc_size)]
    pub mc: Vec<CardPosition<true, true, false>>,
    #[bw(calc(ps.len() as u8))]
    ps_size: u8,
    #[br(count = ps_size)]
    pub ps: Vec<CardPosition<true, true, false>>,
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 37)]
#[repr(C)]
pub struct ReverseDeck;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 38)]
#[repr(C)]
pub struct DeckTop;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 39)]
#[repr(C)]
pub struct ShuffleExtra {
    pub player: LocalPlayer,
    pub count: u8,
    #[br(parse_with=until_eof)]
    pub need_fix: Vec<u32>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 40)]
#[repr(C)]
pub struct NewTurn {
    pub player: LocalPlayer 
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 41)]
#[repr(C)]
pub struct NewPhase {
    pub phase: crate::constants::Phase,
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 42)]
#[repr(C)]
pub struct ConfirmExtraTop {
    pub player: LocalPlayer,
    #[bw(calc(selectable_cards.len() as u8))]
    selectable_cards_size: u8,
    #[br(count = selectable_cards_size)]
    pub selectable_cards: Vec<i32>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 50)]
#[repr(C)]
pub struct Move {
    pub code: i32,
    pub previous: (CardPosition<false, false, false>, Position),
    pub current: (CardPosition<false, false, false>, Position),
    pub reason: crate::constants::Reason
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 53)]
#[repr(C)]
pub struct PositionChange {
    pub card: u32,
    pub controller: Netplayer,
    pub location: Location,
    pub sequence: i8,
    pub previous_position: Position,
    pub current_position: Position
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 54)]
#[repr(C)]
pub struct Set {
    pub position: (CardPosition<true, false, false>, Position)
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 55)]
#[repr(C)]
pub struct Swap {
    pub position1: (CardPosition<true, false, false>, Position),
    pub position2: (CardPosition<true, false, false>, Position)
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 56)]
#[repr(C)]
pub struct FieldDisabled {
    pub disabled: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 60)]
#[repr(C)]
pub struct Summoning {
    pub position: (CardPosition<true, false, false>, Position),
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 61)]
#[repr(C)]
pub struct Summoned;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 62)]
#[repr(C)]
pub struct SpecialSummoning {
    pub position: (CardPosition<true, false, false>, Position),
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 63)]
#[repr(C)]
pub struct Spsummoned;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 64)]
#[repr(C)]
pub struct Flipsummoning {
    pub position: (CardPosition<true, false, false>, Position), 
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 65)]
#[repr(C)]
pub struct Flipsummoned;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 70)]
#[repr(C)]
pub struct Chaining {
    pub card: u32,
    pub previous: CardPosition<false, true, false>,
    pub current: CardPosition<false, false, true>,
    pub target: i8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 71)]
#[repr(C)]
pub struct Chained {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 72)]
#[repr(C)]
pub struct ChainSolving {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 73)]
#[repr(C)]
pub struct ChainSolved {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 74)]
#[repr(C)]
pub struct ChainEnd;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 75)]
#[repr(C)]
pub struct ChainNegated {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 76)]
#[repr(C)]
pub struct ChainDisabled {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 80)]
#[repr(C)]
pub struct CardSelected;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 81)]
#[repr(C)]
pub struct RandomSelected {
    pub player: Netplayer,
    #[bw(calc(pcards.len() as u8))]
    pcards_size: u8,
    #[br(count = pcards_size)]
    pub pcards: Vec<CardPosition<false, true, false>>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 83)]
#[repr(C)]
pub struct BecomeTarget {
    #[bw(calc(pcards.len() as u8))]
    pcards_size: u8,
    #[br(count = pcards_size)]
    pub pcards: Vec<CardPosition<false, true, false>>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 90)]
#[repr(C)]
pub struct Draw {
    pub player: Netplayer,
    #[bw(calc(codes.len() as u8))]
    codes_size: u8,
    #[br(count = codes_size)]
    pub codes: Vec<u32>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 91)]
#[repr(C)]
pub struct Damage {
    pub player: LocalPlayer,
    pub value: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 92)]
#[repr(C)]
pub struct Recover {
    pub player: Netplayer,
    pub value: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 93)]
#[repr(C)]
pub struct Equip {
    pub position1: CardPosition<false, true, false>,
    pub position2: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 94)]
#[repr(C)]
pub struct Lpupdate {
    pub player: Netplayer,
    pub lp: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 95)]
#[repr(C)]
pub struct Unequip {
    pub position1: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 96)]
#[repr(C)]
pub struct CardTarget {
    pub position1: CardPosition<false, true, false>,
    pub position2: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 97)]
#[repr(C)]
pub struct CancelTarget {
    pub position1: CardPosition<false, true, false>,
    pub position2: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 100)]
#[repr(C)]
pub struct PayLpcost {
    pub player: Netplayer,
    pub cost: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 101)]
#[repr(C)]
pub struct AddCounter {
    pub _type: i16,
    pub position: CardPosition<false, false, false>,
    pub count: i16
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 102)]
#[repr(C)]
pub struct RemoveCounter {
    pub _type: i16,
    pub position: CardPosition<false, false, false>,
    pub count: i16
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 110)]
#[repr(C)]
pub struct Attack {
    pub attacker: CardPosition<false, true, false>,
    pub defenser: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 111)]
#[repr(C)]
pub struct Battle {
    pub attacker: CardPosition<false, true, false>,
    pub attacker_attack: i32,
    pub attacker_defense: i32,
    pub denfenser_a: i8, // ???
    pub defenser: CardPosition<false, true, false>,
    pub defenser_attack: i32,
    pub defenser_defense: i32,
    pub defenser_d: i8 // ???
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 112)]
#[repr(C)]
pub struct AttackDisabled;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 113)]
#[repr(C)]
pub struct DamageStepStart;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 114)]
#[repr(C)]
pub struct DamageStepEnd;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 120)]
#[repr(C)]
pub struct MissedEffect {
    pub unknown: i32,
    pub code: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 121)]
#[repr(C)]
pub struct BeChainTarget;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 122)]
#[repr(C)]
pub struct CreateRelation;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 123)]
#[repr(C)]
pub struct ReleaseRelation;

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 130)]
#[repr(C)]
pub struct TossCoin {
    pub player: LocalPlayer,
    #[bw(calc(result.len() as u8))]
    result_size: u8,
    #[br(count = result_size)]
    pub result: Vec<i8>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 131)]
#[repr(C)]
pub struct TossDice {
    pub player: LocalPlayer,
    #[bw(calc(result.len() as u8))]
    result_size: u8,
    #[br(count = result_size)]
    pub result: Vec<i8>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 132)]
#[repr(C)]
pub struct RockPaperScissors {
    pub player: LocalPlayer
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 133)]
#[repr(C)]
pub struct HandResult {
    pub result: i8
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 140)]
#[repr(C)]
pub struct AnnounceRace {
    pub player: LocalPlayer,
    pub annount_count: i8,
    pub available: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 141)]
#[repr(C)]
pub struct AnnounceAttribute {
    pub player: LocalPlayer,
    pub annount_count: i8,
    pub available: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 142)]
#[repr(C)]
pub struct AnnounceCard {
    pub player: LocalPlayer,
    #[bw(calc(value.len() as u8))]
    value_size: u8,
    #[br(count = value_size)]
    pub value: Vec<i32>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 143)]
#[repr(C)]
pub struct AnnounceNumber {
    pub player: LocalPlayer,
    #[bw(calc(value.len() as u8))]
    value_size: u8,
    #[br(count = value_size)]
    pub value: Vec<i32>
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 160)]
#[repr(C)]
pub struct CardHint {
    pub position: CardPosition<false, true, false>,
    pub card_hint_type: i8,
    pub value: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 161)]
#[repr(C)]
pub struct TagSwap {
    pub player: LocalPlayer,
    pub mcount: u8,
    pub ecount: u8,
    pub pcount: u8,
    pub hcount: u8,
    pub topcode: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 162)]
#[repr(C)]
pub struct ReloadField {
    pub duel_rule: u8,
    pub player1_lp: i32,
    
    #[br(parse_with=until_eof)]
    pub data: Vec<u8> // gugugu
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 163)]
#[repr(C)]
pub struct AIName {
    pub name: U16String
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 164)]
#[repr(C)]
pub struct ShowHint {
    pub name: U16String
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 165)]
#[repr(C)]
pub struct PlayerHint {
    pub player: LocalPlayer,
    pub player_hint_type: i8,
    pub value: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 170)]
#[repr(C)]
pub struct MatchKill {
    pub reason: i32
}

#[binrw]
#[derive(Debug, Message)]
#[message(gm, flag = 180)]
#[repr(C)]
pub struct CustomMsg {
    #[br(parse_with=until_eof)]
    pub data: Vec<u8>
}

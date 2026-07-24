#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]

use binrw::BinRead;
use binrw::BinWrite;
use binrw::binrw;
use binrw::helpers::until_eof;
use modular_bitfield::bitfield;
use modular_bitfield::specifiers::B3;
use modular_bitfield::specifiers::B28;
use num_enum::TryFromPrimitive;
use ygopro_derive::Mask;
use ygopro_derive::Message;

use crate::constants::*;
use crate::data::CardPosition;
use crate::data::UpdateCardInfo;
use crate::utils::string::U16String;

include!(concat!(env!("OUT_DIR"), "/game_message.rs"));
every_game_message_flat_message!(crate::generate_enum);

pub trait Mask {
    fn mask(&mut self);
    fn mask_towards(&mut self, _player: CorePlayer) {
        self.mask();
    }
}

impl<T: Mask> Mask for Vec<T> {
    fn mask(&mut self) {
        for item in self { item.mask(); }
    }
    fn mask_towards(&mut self, player: CorePlayer) {
        for item in self { item.mask_towards(player); }
    }
}

pub trait MaskedClone: Mask + Clone {
    fn clone_masked(&self) -> Self {
        let mut mirror = self.clone();
        mirror.mask();
        mirror
    }
}

impl<T> MaskedClone for T where T: Mask + Clone {}

macro_rules! impl_mask_for_message {
    ($($message_name:ident=$message_flag:literal),*) => {
        impl Mask for Message {
            fn mask(&mut self) {
                match self {
                    $(Message::$message_name(inner) => inner.mask()),*
                }
            }
            fn mask_towards(&mut self, player: CorePlayer) {
                match self {
                    $(Message::$message_name(inner) => inner.mask_towards(player)),*
                }
            }
        }
    };
}
every_game_message_flat_message!(impl_mask_for_message);

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 1)]
#[repr(C)] 
pub struct Retry;

#[derive(Debug, Message, Clone, Mask)]
#[message(gm, flag = 2)]
#[binrw]
#[repr(C)]
pub struct Hint {
    pub _type: crate::constants::Hint,
    pub player: CorePlayer,
    pub data: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 3)]
#[repr(C)]
pub struct Waiting;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 4)]
#[repr(C)]
pub struct Start {
    pub player_type: u8,
    pub rule: MasterRule,
    pub player1_lp: i32,
    pub player2_lp: i32,
    pub player1_deck_count: u16,
    pub player1_extra_count: u16,
    pub player2_deck_count: u16,
    pub player2_extra_count: u16
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 5)]
#[repr(C)]
pub struct Win {
    pub winner: CorePlayer,
    pub reason: WinReason
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 6)]
#[repr(C)]
pub struct UpdateData {
    pub player: CorePlayer,
    pub location: Location,
    #[br(parse_with=until_eof)]
    pub data: Vec<UpdateCardInfo>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 7)]
#[repr(C)]
pub struct UpdateCard {
    pub position: CardPosition<false, false, false>,
    pub data: UpdateCardInfo
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 8)]
#[repr(C)]
pub struct RequestDeck;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 10)]
#[repr(C)]
pub struct SelectBattleCommand {
    pub selecting_player: CorePlayer,
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
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 11)]
#[repr(C)]
pub struct SelectIdleCommand {
    pub selecting_player: CorePlayer,
    #[bw(calc(summonable_cards.len() as u8))]
    summonable_cards_size: u8,
    #[br(count = summonable_cards_size)]
    pub summonable_cards: Vec<CardPosition<true, false, false>>,
    #[bw(calc(special_summonable_cards.len() as u8))]
    special_summonable_cards_size: u8,
    #[br(count = special_summonable_cards_size)]
    pub special_summonable_cards: Vec<CardPosition<true, false, false>>,
    #[bw(calc(repositionable_cards.len() as u8))]
    repositionable_cards_size: u8,
    #[br(count = repositionable_cards_size)]
    pub repositionable_cards: Vec<CardPosition<true, false, false>>,
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
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 12)]
#[repr(C)]
pub struct SelectEffectYesNo {
    pub selecting_player: CorePlayer,
    pub card_position: CardPosition<true, true, true>,
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 13)]
#[repr(C)]
pub struct SelectYesNo {
    pub selecting_player: CorePlayer,
    pub description: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 14)]
#[repr(C)]
pub struct SelectOption {
    pub selecting_player: CorePlayer,
    #[bw(calc(options.len() as u8))]
    options_size: u8,
    #[br(count = options_size)]
    pub options: Vec<i32>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 15)]
#[repr(C)]
pub struct SelectCard {
    pub selecting_player: CorePlayer,
    #[br(map=|v:u8| v>0)]
    #[bw(map=|v| if *v {1u8} else {0u8})]
    pub select_cancelable: bool,
    pub select_min: i8,
    pub select_max: i8,
    #[bw(calc(positions.len() as u8))]
    positions_size: u8,
    #[br(count = positions_size)]
    #[mask]
    pub positions: Vec<CardPosition<true, true, false>>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 16)]
#[repr(C)]
pub struct SelectChain {
    pub selecting_player: CorePlayer,
    #[bw(calc(activatable_cards.len() as u8))]
    pub activatable_cards_count: u8,
    pub special_count: u8,
    #[br(map = |v: u8| v != 0)]
    #[bw(map = |v: &bool| *v as u8)]
    pub forced: bool,
    pub hint0: i32,
    pub hint1: i32,
    #[br(count = activatable_cards_count)]
    pub activatable_cards: Vec<(i8, CardPosition<true, true, true>)>,
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 18)]
#[repr(C)]
pub struct SelectPlace {
    pub selecting_player: CorePlayer,
    pub count: i8,
    pub selectable_field: i32,
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 19)]
#[repr(C)]
pub struct SelectPosition {
    pub selecting_player: CorePlayer,
    pub code: u32,
    pub positions: Position
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 20)]
#[repr(C)]
pub struct SelectTribute {
    pub selecting_player: CorePlayer,
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
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 21)]
#[repr(C)]
pub struct SortChain;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 22)]
#[repr(C)]
pub struct SelectCounter {
    pub selecting_player: CorePlayer,
    pub select_counter_type: i16,
    pub select_counter_count: i16,
    #[bw(calc(selectable_cards.len() as u8))]
    selectable_cards_size: u8,
    #[br(count = selectable_cards_size)]
    pub selectable_cards: Vec<(CardPosition<true, true, false>, i8)> // OpParam
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 23)]
#[repr(C)]
pub struct SelectSum {
    pub select_mode: SelectSumMode,
    pub selecting_player: CorePlayer,
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
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 24)]
#[repr(C)]
pub struct SelectDisableField {
    pub selecting_player: CorePlayer,
    pub count: i8,
    pub selectable_field: i32,
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 25)]
#[repr(C)]
pub struct SortCard {
    pub player: CorePlayer,
    #[bw(calc(cards.len() as u8))]
    cards_size: u8,
    #[br(count = cards_size)]
    pub cards: Vec<CardPosition<true, false, false>>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 26)]
#[repr(C)]
pub struct SelectUnselectCard {
    pub selecting_player: CorePlayer,
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
    #[mask]
    pub positions1: Vec<CardPosition<true, true, false>>,
    #[bw(calc(positions2.len() as u8))]
    positions2_size: u8,
    #[br(count = positions2_size)]
    #[mask]
    pub positions2: Vec<CardPosition<true, true, false>>
}

#[binrw]
#[derive(Clone, Debug, Message, Mask)]
#[message(gm, flag = 30)]
pub struct ConfirmDecktop {
    pub controller: CorePlayer,
    #[bw(calc(codes.len() as u8))]
    codes_size: u8,
    #[br(count = codes_size)]
    pub codes: Vec<i32>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 31)]
#[repr(C)]
pub struct ConfirmCards {
    pub player: CorePlayer,
    #[bw(calc(cards.len() as u8))]
    cards_size: u8,
    #[br(count = cards_size)]
    pub cards: Vec<CardPosition<true, false, false>>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 32)]
#[repr(C)]
pub struct ShuffleDeck {
    pub player: CorePlayer 
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 33)]
#[repr(C)]
pub struct ShuffleHand {
    pub player: CorePlayer,
    pub count: u8,
    #[br(count = count)]
    #[mask]
    #[mask_if(self.player != player)]
    pub codes: Vec<CardCode>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 34)]
#[repr(C)]
pub struct RefreshDeck {
    pub player: CorePlayer
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 35)]
#[repr(C)]
pub struct SwapGraveDeck {
    pub player: CorePlayer 
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
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
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 37)]
#[repr(C)]
pub struct ReverseDeck;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 38)]
#[repr(C)]
pub struct DeckTop;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 39)]
#[repr(C)]
pub struct ShuffleExtra {
    pub player: CorePlayer,
    pub count: u8,
    #[br(count = count)]
    #[mask]
    #[mask_if(self.player != player)]
    pub codes: Vec<CardCode>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 40)]
#[repr(C)]
pub struct NewTurn {
    pub player: CorePlayer 
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 41)]
#[repr(C)]
pub struct NewPhase {
    pub phase: crate::constants::Phase,
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 42)]
#[repr(C)]
pub struct ConfirmExtraTop {
    pub player: CorePlayer,
    #[bw(calc(selectable_cards.len() as u8))]
    selectable_cards_size: u8,
    #[br(count = selectable_cards_size)]
    pub selectable_cards: Vec<i32>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 50)]
#[repr(C)]
pub struct Move {
    #[mask]
    #[mask_if(self.current.0.controller != player && !self.current.0.location.intersects(Location::Grave | Location::Overlay) && (self.current.0.location.intersects(Location::Deck | Location::Hand) || self.current.1.is_face_down()))]
    pub code: i32,
    pub previous: (CardPosition<false, false, false>, Position),
    pub current: (CardPosition<false, false, false>, Position),
    pub reason: crate::constants::Reason
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 53)]
#[repr(C)]
pub struct PositionChange {
    pub card: u32,
    pub controller: CorePlayer,
    pub location: Location,
    pub sequence: i8,
    pub previous_position: Position,
    pub current_position: Position
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 54)]
#[repr(C)]
pub struct Set {
    #[mask]
    pub code: i32,
    pub position: (CardPosition<false, false, false>, Position)
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 55)]
#[repr(C)]
pub struct Swap {
    pub position1: (CardPosition<true, false, false>, Position),
    pub position2: (CardPosition<true, false, false>, Position)
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 56)]
#[repr(C)]
pub struct FieldDisabled {
    pub disabled: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 60)]
#[repr(C)]
pub struct Summoning {
    pub position: (CardPosition<true, false, false>, Position),
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 61)]
#[repr(C)]
pub struct Summoned;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 62)]
#[repr(C)]
pub struct SpecialSummoning {
    #[mask]
    #[mask_if(self.position.1.is_face_down())]
    pub position: (CardPosition<true, false, false>, Position),
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 63)]
#[repr(C)]
pub struct SpecialSummoned;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 64)]
#[repr(C)]
pub struct FlipSummoning {
    pub position: (CardPosition<true, false, false>, Position), 
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 65)]
#[repr(C)]
pub struct FlipSummoned;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 70)]
#[repr(C)]
pub struct Chaining {
    pub card: u32,
    pub previous: CardPosition<false, true, false>,
    pub current: CardPosition<false, false, true>,
    pub target: i8
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 71)]
#[repr(C)]
pub struct Chained {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 72)]
#[repr(C)]
pub struct ChainSolving {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 73)]
#[repr(C)]
pub struct ChainSolved {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 74)]
#[repr(C)]
pub struct ChainEnd;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 75)]
#[repr(C)]
pub struct ChainNegated {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 76)]
#[repr(C)]
pub struct ChainDisabled {
    pub chain_index: i8
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 80)]
#[repr(C)]
pub struct CardSelected;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 81)]
#[repr(C)]
pub struct RandomSelected {
    pub player: CorePlayer,
    #[bw(calc(pcards.len() as u8))]
    pcards_size: u8,
    #[br(count = pcards_size)]
    pub pcards: Vec<CardPosition<false, true, false>>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 83)]
#[repr(C)]
pub struct BecomeTarget {
    #[bw(calc(pcards.len() as u8))]
    pcards_size: u8,
    #[br(count = pcards_size)]
    pub pcards: Vec<CardPosition<false, true, false>>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 90)]
#[repr(C)]
pub struct Draw {
    pub player: CorePlayer,
    #[bw(calc(codes.len() as u8))]
    codes_size: u8,
    #[br(count = codes_size)]
    #[mask]
    #[mask_if(self.player != player)]
    pub codes: Vec<CardCode>
}

#[bitfield]
#[derive(BinRead, BinWrite, Debug, Copy, Clone, PartialEq, Eq, Default)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
#[repr(u32)]
pub struct CardCode {
    pub id: B28,
    pub _padding: B3,
    pub is_public: bool,
}

impl Mask for CardCode {
    fn mask(&mut self) {
        self.set_id(0);
    }
    fn mask_towards(&mut self, _player: CorePlayer) {
        if !self.is_public() {
            self.set_id(0);
        }
    }
}

impl<T: Mask> Mask for (T,) {
    fn mask(&mut self) {
        self.0.mask();
    }
    fn mask_towards(&mut self, player: CorePlayer) {
        self.0.mask_towards(player);
    }
}

impl<T: Mask, U> Mask for (T, U) {
    fn mask(&mut self) {
        self.0.mask();
    }
    fn mask_towards(&mut self, player: CorePlayer) {
        self.0.mask_towards(player);
    }
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 91)]
#[repr(C)]
pub struct Damage {
    pub player: CorePlayer,
    pub value: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 92)]
#[repr(C)]
pub struct Recover {
    pub player: CorePlayer,
    pub value: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 93)]
#[repr(C)]
pub struct Equip {
    pub position1: CardPosition<false, true, false>,
    pub position2: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 94)]
#[repr(C)]
pub struct LPUpdate {
    pub player: CorePlayer,
    pub lp: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 95)]
#[repr(C)]
pub struct Unequip {
    pub position1: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 96)]
#[repr(C)]
pub struct CardTarget {
    pub position1: CardPosition<false, true, false>,
    pub position2: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 97)]
#[repr(C)]
pub struct CancelTarget {
    pub position1: CardPosition<false, true, false>,
    pub position2: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 100)]
#[repr(C)]
pub struct PayLPCost {
    pub player: CorePlayer,
    pub cost: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 101)]
#[repr(C)]
pub struct AddCounter {
    pub _type: i16,
    pub position: CardPosition<false, false, false>,
    pub count: i16
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 102)]
#[repr(C)]
pub struct RemoveCounter {
    pub _type: i16,
    pub position: CardPosition<false, false, false>,
    pub count: i16
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 110)]
#[repr(C)]
pub struct Attack {
    pub attacker: CardPosition<false, true, false>,
    pub defenser: CardPosition<false, true, false>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 111)]
#[repr(C)]
pub struct Battle {
    pub attacker: CardPosition<false, true, false>,
    pub attacker_attack: i32,
    pub attacker_defense: i32,
    #[br(map = |v: u8| v != 0)]
    #[bw(map = |v: &bool| *v as u8)]
    pub attacker_destroyed: bool,
    pub defenser: CardPosition<false, true, false>,
    pub defenser_attack: i32,
    pub defenser_defense: i32,
    #[br(map = |v: u8| v != 0)]
    #[bw(map = |v: &bool| *v as u8)]
    pub defender_destroyed: bool,
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 112)]
#[repr(C)]
pub struct AttackDisabled;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 113)]
#[repr(C)]
pub struct DamageStepStart;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 114)]
#[repr(C)]
pub struct DamageStepEnd;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 120)]
#[repr(C)]
pub struct MissedEffect {
    pub unknown: i32,
    pub code: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 121)]
#[repr(C)]
pub struct BeChainTarget;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 122)]
#[repr(C)]
pub struct CreateRelation;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 123)]
#[repr(C)]
pub struct ReleaseRelation;

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 130)]
#[repr(C)]
pub struct TossCoin {
    pub player: CorePlayer,
    #[bw(calc(result.len() as u8))]
    result_size: u8,
    #[br(count = result_size)]
    pub result: Vec<i8>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 131)]
#[repr(C)]
pub struct TossDice {
    pub player: CorePlayer,
    #[bw(calc(result.len() as u8))]
    result_size: u8,
    #[br(count = result_size)]
    pub result: Vec<i8>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 132)]
#[repr(C)]
pub struct RockPaperScissors {
    pub player: CorePlayer
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 133)]
#[repr(C)]
pub struct HandResult {
    #[br(temp)]
    #[bw(calc = u8::from(*hand0) | (u8::from(*hand1) << 2))]
    _packed: u8,
    #[br(calc = Hand::try_from_primitive(_packed & 0x03).unwrap())]
    #[bw(ignore)]
    pub hand0: Hand,
    #[br(calc = Hand::try_from_primitive((_packed >> 2) & 0x03).unwrap())]
    #[bw(ignore)]
    pub hand1: Hand,
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 140)]
#[repr(C)]
pub struct AnnounceRace {
    pub player: CorePlayer,
    pub announce_count: i8,
    pub available: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 141)]
#[repr(C)]
pub struct AnnounceAttribute {
    pub player: CorePlayer,
    pub announce_count: i8,
    pub available: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 142)]
#[repr(C)]
pub struct AnnounceCard {
    pub player: CorePlayer,
    #[bw(calc(value.len() as u8))]
    value_size: u8,
    #[br(count = value_size)]
    pub value: Vec<i32>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 143)]
#[repr(C)]
pub struct AnnounceNumber {
    pub player: CorePlayer,
    #[bw(calc(value.len() as u8))]
    value_size: u8,
    #[br(count = value_size)]
    pub value: Vec<i32>
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 160)]
#[repr(C)]
pub struct CardHint {
    pub position: CardPosition<false, true, false>,
    pub card_hint_type: crate::constants::CardHint,
    pub value: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 161)]
#[repr(C)]
pub struct TagSwap {
    pub player: CorePlayer,
    pub mcount: u8,
    pub ecount: u8,
    pub pcount: u8,
    pub hcount: u8,
    pub top_code: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 162)]
#[repr(C)]
pub struct ReloadField {
    pub duel_rule: MasterRule,
    pub player1_lp: i32,
    
    #[br(parse_with=until_eof)]
    pub data: Vec<u8> // gugugu
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 163)]
#[repr(C)]
pub struct AIName {
    pub name: U16String
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 164)]
#[repr(C)]
pub struct ShowHint {
    pub name: U16String
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 165)]
#[repr(C)]
pub struct PlayerHint {
    pub player: CorePlayer,
    pub player_hint_type: crate::constants::PlayerHint,
    pub value: i32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 170)]
#[repr(C)]
pub struct MatchKill {
    pub card_code: u32
}

#[binrw]
#[derive(Debug, Clone, Message, Mask)]
#[message(gm, flag = 180)]
#[repr(C)]
pub struct CustomMsg {
    #[br(parse_with=until_eof)]
    pub data: Vec<u8>
}

#[cfg(test)]
mod test {
    #[test]
    fn print_sizes() {
        macro_rules! print_size {
            ($($msg:ident = $flag:literal),* $(,)?) => {
                println!("=== GM ===");
                $(
                    println!("  {:30}: {:>4} bytes", stringify!($msg), std::mem::size_of::<super::$msg>());
                )*
                println!("  {:30}: {:>4} bytes", "MessageType", std::mem::size_of::<super::MessageType>());
                println!("  {:30}: {:>4} bytes", "Message", std::mem::size_of::<super::Message>());
            };
        }
        every_game_message_flat_message!(print_size);
    }
}

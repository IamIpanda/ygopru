//! A deck of cards.
//!
//! Provides the [`Deck`] container and the deck-parsing logic.

use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Display;
use std::str::FromStr;

use binrw::BinRead;
use binrw::BinWrite;
use binrw::binrw;
use modular_bitfield::Specifier;
use modular_bitfield::bitfield;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;

use crate::constants::OT;
use crate::constants::Rule;
use crate::constants::Type;
use crate::data::Card;
use crate::data::LFList;

const DECK_MIN: usize = 40;
const DECK_MAX: usize = 60;
const EXTRA_MAX: usize = 15;
const SIDE_MAX: usize = 15;

/// Player Deck.
#[binrw]
#[derive(Debug, Clone, Default)]
pub struct Deck {
    #[bw(calc = main.len() as u32 + extra.len() as u32)]
    main_size: u32,
    #[bw(calc = side.len() as u32)]
    side_size: u32,
    #[br(count = main_size)]
    pub main: Vec<u32>,
    #[br(count = side_size)]
    pub side: Vec<u32>,
    #[br(ignore)]
    pub extra: Vec<u32>,
}

impl Deck {
    /// Create an empty deck.
    pub fn new() -> Self { Self::default() }

    /// Build a deck from raw card codes, splitting into main and side.
    pub fn load_from_codes(codes: &[u32], mainc: usize, sidec: usize) -> Self {
        let mut d = Self::new();
        let mc = mainc.min(codes.len());
        d.main.extend_from_slice(&codes[..mc]);
        let sc = sidec.min(codes.len().saturating_sub(mc));
        d.side.extend_from_slice(&codes[mc..mc + sc]);
        d
    }

    /// Count each card code across main, extra, and side.
    pub fn get_hash(&self) -> HashMap<u32, usize> {
        let mut counts: HashMap<u32, usize> = HashMap::new();
        for &code in self.main.iter().chain(self.extra.iter()).chain(self.side.iter()) {
            *counts.entry(code).or_insert(0) += 1;
        }
        counts 
    }

    /// Drop unknown/token cards and move extra-deck cards into the extra.
    pub fn load<'a>(&mut self, resolve_card: impl Fn(u32) -> Option<&'a Card>) -> Option<DeckError> {
        let response = remove_unknown_cards(&mut self.main, |c| resolve_card(c).map(|c| c.card_type))
            .or(remove_unknown_cards(&mut self.side, |c| resolve_card(c).map(|c| c.card_type)));
        self.separate(|c| resolve_card(c).map(|c| c.card_type).unwrap_or(Type::empty()));
        response
    }

    /// Check the deck against the limit list and the rule.
    pub fn prepare<'a>(&mut self, lflist: &LFList, rule: Rule, resolve_card: impl Fn(u32) -> Option<&'a Card>) -> Result<(), DeckError> {
        self.check(lflist, rule, 
            |c| resolve_card(c).map(|c| c.ot).unwrap_or(OT::empty()), 
            |c| resolve_card(c).map(|c| c.card_type).unwrap_or(Type::empty()),
            |c| resolve_card(c).map(|c| c.duel_code()).unwrap_or(0))
    }

    /// Check a deck after replacing side, comparing against this deck.
    pub fn check_after_replacing_side<'a>(&self, deck: &mut Deck, resolve_card: impl Fn(u32) -> Option<&'a Card>) -> Result<(), DeckError> {
        deck.separate(|c| resolve_card(c).map(|c| c.card_type).unwrap_or(Type::empty()));
        if self == deck {
            Ok(())
        } else {
            Err(DeckError::new().with_error_type(DeckErrorType::SideCount))
        }
    }

    /// Move extra-deck cards from main into the extra.
    pub fn separate(&mut self, resolve_type: impl Fn(u32) -> Type) {
        separate_main_and_extra(&mut self.main, &mut self.extra, resolve_type);
    }

    /// Run all the deck checks (length, illegal cards, rule, limit list).
    pub fn check(&self, lflist: &LFList, rule: Rule, get_rule: impl Fn(u32) -> OT, get_type: impl Fn(u32) -> Type, resolve_code: impl Fn(u32) -> u32) -> Result<(), DeckError> {
        check_deck_length(&self.main, &self.extra, &self.side)?;
        check_illegal_cards(&self.main, &self.side, &self.extra, get_type)?;
        let iter = self.main.iter().chain(self.extra.iter()).chain(self.side.iter());
        check_rule(iter.clone(), rule, get_rule)?;
        check_deck_lflists(iter, lflist, resolve_code)
    }
}

impl ToString for Deck {
    fn to_string(&self) -> String {
        let mut text = String::from("#ygopro-rs deck generated\n#main\n");
        for code in &self.main {
            text.push_str(&code.to_string());
            text.push('\n');
        }
        if self.extra.len() > 0 {
            text.push_str("#extra\n");
            for code in &self.extra {
                text.push_str(&code.to_string());
                text.push('\n');
            }
        }
        text.push_str("!side\n");
        for code in &self.side {
            text.push_str(&code.to_string());
            text.push('\n');
        }
        text
    }
}

impl FromStr for Deck {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut deck = Self::new();
        let mut section = &mut deck.main;
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            match line.as_bytes()[0] {
                b'!' => section = &mut deck.side,
                b'#' => match line {
                    "#main" => section = &mut deck.main,
                    "#extra" => section = &mut deck.extra,
                    _ => {}
                },
                b'0'..=b'9' => {
                    let code_end = line.find(|c: char| !c.is_ascii_digit()).unwrap_or(line.len());
                    if let Ok(code) = line[..code_end].parse::<u32>() {
                        section.push(code);
                    }
                }
                _ => {}
            }
        }
        Ok(deck)
    }
}

impl PartialEq for Deck {
    fn eq(&self, other: &Self) -> bool {
        if self.main.len() != other.main.len() 
            || self.side.len() != other.side.len()
            || self.extra.len() != other.extra.len() {
            return false;
        }
        self.get_hash() == other.get_hash()
    }
}

impl Eq for Deck {}

/// The kind of deck error.
#[derive(Specifier, Clone, Copy, Debug, IntoPrimitive, TryFromPrimitive, PartialEq, Eq)]
#[bits = 4]
#[repr(u8)]
pub enum DeckErrorType {
    /// Violates the limit list.
    Lflist = 0x1,
    /// Contains card only available in OCG.
    OcgOnly = 0x2,
    /// Contains card only available in TCG.
    TcgOnly = 0x3,
    /// Contains unknown card.
    UnknownCard = 0x4,
    /// Too many copies of a card.
    CardCount = 0x5,
    /// The main deck size is wrong.
    MainCount = 0x6,
    /// The extra deck size is wrong.
    ExtraCount = 0x7,
    /// The side deck size is wrong.
    SideCount = 0x8,
    /// A card is not available.
    NotAvailable = 0x9,
}

/// A deck error, packing an error type and the offending card code.
#[bitfield]
#[derive(BinRead, BinWrite, Debug, Clone, Copy, PartialEq, Eq)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
#[repr(u32)]
pub struct DeckError {
    /// The offending card code.
    pub code: modular_bitfield::specifiers::B28,
    /// The error type.
    pub error_type: DeckErrorType,
}

impl Display for DeckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DeckError({:?}, code: {})", self.error_type(), self.code())
    }
}

impl std::error::Error for DeckError {}

const EXTRA_TYPE: Type = Type::from_bits_retain(0x4802040);

/// Move extra-deck cards from `main` into `ex`.
pub fn separate_main_and_extra(main: &mut Vec<u32>, ex: &mut Vec<u32>, resolve_type: impl Fn(u32) -> Type) {
    main.retain(|&code| {
        if resolve_type(code).intersects(EXTRA_TYPE) {
            if ex.len() < EXTRA_MAX { ex.push(code); }
            false
        } else {
            true
        }
    });
}

/// Check the main, extra, and side deck sizes.
pub fn check_deck_length(main: &[u32], extra: &[u32], side: &[u32]) -> Result<(),DeckError> {
    if main.len() < DECK_MIN || main.len() > DECK_MAX { return Err(DeckError::new().with_error_type(DeckErrorType::MainCount).with_code(main.len() as u32)); }
    if extra.len() > EXTRA_MAX { return Err(DeckError::new().with_error_type(DeckErrorType::ExtraCount).with_code(extra.len() as u32)); }
    if side.len() > SIDE_MAX { return Err(DeckError::new().with_error_type(DeckErrorType::SideCount).with_code(side.len() as u32)); }
    Ok(())
}

/// Remove unknown or token cards, returning an error for the last removed card.
pub fn remove_unknown_cards(main: &mut Vec<u32>, get_type: impl Fn(u32) -> Option<Type>) -> Option<DeckError> {
    let mut last_removed_code = None;
    main.retain(|code| {
        let _type = get_type(*code);
        if match _type {
            Some(_type) => _type.contains(Type::Token),
            None => true
        } {
            last_removed_code = Some(*code);
            false
        } else { true }
    });
    last_removed_code.map(|code| DeckError::new().with_error_type(DeckErrorType::UnknownCard).with_code(code))
}

/// Check that no illegal card (token, or a card in the wrong section) is present.
pub fn check_illegal_cards(main: &Vec<u32>, side: &Vec<u32>, ex: &Vec<u32>, get_type: impl Fn(u32) -> Type) -> Result<(), DeckError> {
    for code in main {
        let card_type = get_type(*code);
        if card_type.contains(Type::Token) || card_type.intersects(EXTRA_TYPE) {
            return Err(DeckError::new().with_error_type(DeckErrorType::MainCount).with_code(0));
        }
    }
    for code in side {
        if get_type(*code).contains(Type::Token) {
            return Err(DeckError::new().with_error_type(DeckErrorType::SideCount).with_code(0));
        }
    }
    for code in ex {
        let card_type = get_type(*code);
        if card_type.contains(Type::Token) || !card_type.intersects(EXTRA_TYPE) {
            return Err(DeckError::new().with_error_type(DeckErrorType::ExtraCount).with_code(0));
        }
    }
    Ok(())
}

/// Check that every card is allowed by the rule's availability.
pub fn check_rule<'a>(codes: impl Iterator<Item = &'a u32>, rule: Rule, get_rule: impl Fn(u32) -> OT) -> Result<(), DeckError> {
    for &code in codes {
        let ot = get_rule(code);
        if let Some(error_type) = rule.check_ot(ot) {
            return Err(DeckError::new().with_error_type(error_type).with_code(code));
        }
    }
    Ok(())
}

/// Check the card counts against the limit list.
pub fn check_deck_lflists<'a>(codes: impl Iterator<Item = &'a u32>, lflist: &LFList, resolve_code: impl Fn(u32) -> u32) -> Result<(), DeckError> {
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for &code in codes {
        let resolved = resolve_code(code);
        *counts.entry(resolved).or_insert(0) += 1;
    }

    let mut current = 0;
    for (&code, &count) in &counts {
        if count > 3 {
            return Err(DeckError::new().with_error_type(DeckErrorType::CardCount).with_code(code));
        }
        if lflist.genesys > 0 && let Some(&limit) = lflist.glist.get(&code) {
            current += limit * count;
            if current > lflist.genesys {
                return Err(DeckError::new().with_error_type(DeckErrorType::Lflist).with_code(code));
            }
        }
        if let Some(&limit) = lflist.content.get(&code)
            && count as u8 > limit {
                return Err(DeckError::new().with_error_type(DeckErrorType::Lflist).with_code(code));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::data::Deck;

    #[test]
    fn splits_main_and_side_at_bang_marker() {
        let deck: Deck = "#created by test\n#main\n123\n456\n#extra\n789\n!side\n111\n222\n"
            .parse()
            .unwrap();
        assert_eq!(deck.main, vec![123, 456, 789]);
        assert!(deck.extra.is_empty());
        assert_eq!(deck.side, vec![111, 222]);
    }

    #[test]
    fn drops_comment_blank_and_invalid_lines() {
        let deck: Deck = "   \n#comment\n123abc\nnot-a-number\n!side\n\nxyz\n".parse().unwrap();
        assert_eq!(deck.main, vec![123]);
        assert!(deck.side.is_empty());
    }

    #[test]
    fn round_trips_through_to_string() {
        let deck: Deck = "#main\n1\n2\n3\n!side\n4\n".parse().unwrap();
        let text = deck.to_string();
        let reparsed: Deck = text.parse().unwrap();
        assert_eq!(reparsed.main, deck.main);
        assert_eq!(reparsed.extra, deck.extra);
        assert_eq!(reparsed.side, deck.side);
    }
}

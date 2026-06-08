use std::collections::HashMap;

use binrw::binrw;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;

const DECK_MIN: usize = 40;
const DECK_MAX: usize = 60;
const EXTRA_MAX: usize = 15;
const SIDE_MAX: usize = 15;

#[binrw]
#[derive(PartialEq, Eq, Debug, Clone, Default)]
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
    pub fn new() -> Self { Self::default() }

    pub fn load_from_codes(codes: &[u32], mainc: usize, sidec: usize) -> Self {
        let mut d = Self::new();
        let mc = mainc.min(codes.len());
        d.main.extend_from_slice(&codes[..mc]);
        let sc = sidec.min(codes.len().saturating_sub(mc));
        d.side.extend_from_slice(&codes[mc..mc + sc]);
        d
    }
}

pub fn side_from_codes(deck: &mut Deck, codes: &[u32], mainc: usize, sidec: usize) -> bool {
    let mc = mainc.min(codes.len());
    let sc = sidec.min(codes.len().saturating_sub(mc));
    if mc + sc > codes.len() { return false; }
    deck.main.clear();
    deck.main.extend_from_slice(&codes[..mc]);
    deck.side.clear();
    deck.side.extend_from_slice(&codes[mc..mc + sc]);
    true
}

#[binrw]
#[derive(PartialEq, Eq, Debug, Clone, Default)]
pub struct ReplayDeck {
    #[bw(calc = main.len() as u32 + extra.len() as u32)]
    main_size: u32,
    #[br(count = main_size)]
    pub main: Vec<u32>,
    #[bw(calc = side.len() as u32)]
    side_size: u32,
    #[br(count = side_size)]
    pub side: Vec<u32>,
    #[br(ignore)]
    pub extra: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum DeckErrorFlags {
    LFLIST = 0x1,
    OCGONLY = 0x2,
    TCGONLY = 0x3,
    UNKNOWNCARD = 0x4,
    CARDCOUNT = 0x5,
    MAINCOUNT = 0x6,
    EXTRACOUNT = 0x7,
    SIDECOUNT = 0x8,
    NOTAVAIL = 0x9,
}

#[derive(Debug, Clone)]
pub struct DeckCheckError {
    pub flags: DeckErrorFlags,
    pub code: u32,
}

impl std::fmt::Display for DeckCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Deck check error: {:?} (code: {})", self.flags, self.code)
    }
}

impl std::error::Error for DeckCheckError {}

pub fn check_deck(
    main: &[u32],
    extra: &[u32],
    side: &[u32],
    lflist: &HashMap<u32, u8>,
    resolve_code: impl Fn(u32) -> u32,
) -> Result<(), DeckCheckError> {
    if main.len() < DECK_MIN || main.len() > DECK_MAX {
        return Err(DeckCheckError { flags: DeckErrorFlags::MAINCOUNT, code: 0 });
    }
    if extra.len() > EXTRA_MAX {
        return Err(DeckCheckError { flags: DeckErrorFlags::EXTRACOUNT, code: 0 });
    }
    if side.len() > SIDE_MAX {
        return Err(DeckCheckError { flags: DeckErrorFlags::SIDECOUNT, code: 0 });
    }

    let mut counts: HashMap<u32, u32> = HashMap::new();
    for &code in main.iter().chain(extra.iter()) {
        let resolved = resolve_code(code);
        *counts.entry(resolved).or_insert(0) += 1;
    }

    for (&code, &count) in &counts {
        if let Some(&limit) = lflist.get(&code) {
            if count as u8 > limit {
                return Err(DeckCheckError { flags: DeckErrorFlags::LFLIST, code });
            }
        }
        if count > 3 {
            return Err(DeckCheckError { flags: DeckErrorFlags::CARDCOUNT, code });
        }
    }
    Ok(())
}

pub fn encode_deck_error(flags: DeckErrorFlags, code: u32) -> u32 {
    ((flags as u32) << 28) | (code & 0x0FFFFFFF)
}

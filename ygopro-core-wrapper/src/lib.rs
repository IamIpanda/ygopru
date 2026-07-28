// ygopro-core-wrapper: FFI bindings for ocgcore
// 对应: ../ygopro/ocgcore/ocgapi.h

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use modular_bitfield::Specifier;
use modular_bitfield::bitfield;
use modular_bitfield::specifiers::B28;
use parking_lot::Mutex;
use std::ffi::CString;
use std::os::raw::c_char;
use std::os::raw::c_int;

use ygopro_data::constants::CorePlayer;
use ygopro_data::constants::Location;
use ygopro_data::constants::MasterRule;
use ygopro_data::constants::Position;
use ygopro_data::constants::Query;
use ygopro_data::data::CoreCard;
use ygopro_data::data::DuelOptions;

pub type intptr_t = isize;

pub mod random;
pub use random::DuelSeed;
pub type script_reader = Option<extern "C" fn(*const c_char, *mut c_int) -> *mut u8>;
pub type card_reader = Option<extern "C" fn(u32, *mut CoreCard) -> u32>;
pub type message_handler = Option<extern "C" fn(intptr_t, u32) -> u32>;


unsafe extern "C" {
    pub fn set_script_reader(f: script_reader);
    pub fn set_card_reader(f: card_reader);
    pub fn set_message_handler(f: message_handler);

    pub fn create_duel(seed: u32) -> intptr_t;
    pub fn create_duel_v2(seed_sequence: *const u32) -> intptr_t;
    pub fn start_duel(pduel: intptr_t, options: u32);
    pub fn end_duel(pduel: intptr_t);
    pub fn set_player_info(pduel: intptr_t, playerid: i32, lp: i32, startcount: i32, drawcount: i32);
    pub fn get_log_message(pduel: intptr_t, buf: *mut u8);
    pub fn get_message(pduel: intptr_t, buf: *mut u8) -> i32;
    pub fn process(pduel: intptr_t) -> u32;
    pub fn new_card(pduel: intptr_t, code: u32, owner: u8, playerid: u8, location: u8, sequence: u8, position: u8);
    pub fn new_tag_card(pduel: intptr_t, code: u32, owner: u8, location: u8);
    pub fn query_card(pduel: intptr_t, playerid: u8, location: u8, sequence: u8, query_flag: u32, buf: *mut u8, use_cache: i32) -> i32;
    pub fn query_field_count(pduel: intptr_t, playerid: u8, location: u8) -> i32;
    pub fn query_field_card(pduel: intptr_t, playerid: u8, location: u8, query_flag: u32, buf: *mut u8, use_cache: i32) -> i32;
    pub fn query_field_info(pduel: intptr_t, buf: *mut u8) -> i32;
    pub fn set_responsei(pduel: intptr_t, value: i32);
    pub fn set_responseb(pduel: intptr_t, buf: *mut u8);
    pub fn preload_script(pduel: intptr_t, script_name: *const c_char) -> i32;
}

pub struct Duel {
    duel_pointer: intptr_t,
    shuffler: random::MTRandom,
    ended: bool
}

static DUEL_LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());

impl Duel {
    pub fn new(seed: DuelSeed) -> Self {
        let _guard = DUEL_LIFECYCLE_LOCK.lock();
        let (duel_pointer, seed_array) = match seed {
            DuelSeed::None => {
                let mut seeds = [0; 8];
                for i in 0..8 {
                    seeds[i] = rand::random();
                }
                (unsafe { create_duel_v2(seeds.as_ptr()) }, seeds)
            },
            DuelSeed::Single(s) => (unsafe { create_duel(s) }, [s; 8]),
            DuelSeed::Complicated(seq) => (unsafe { create_duel_v2(seq.as_ptr()) }, seq),
        };
        let shuffler = random::MTRandom::new(DuelSeed::Complicated(seed_array));
        Self { duel_pointer, shuffler, ended: false }
    }

    pub fn start(&self, options: DuelOptions, rule: MasterRule) {
        let opt = ((rule as u32) << 16) | (options.bits() as u32);
        unsafe { start_duel(self.duel_pointer, opt) };
    }

    pub fn end(&mut self) {
        if self.ended { return }
        let _guard = DUEL_LIFECYCLE_LOCK.lock();
        unsafe { end_duel(self.duel_pointer) };
        self.ended = true;
    }

    pub fn set_player_info(&self, player: CorePlayer, lp: i32, start_count: i32, draw_count: i32) {
        unsafe { set_player_info(self.duel_pointer, player as i32, lp, start_count, draw_count) };
    }

    pub fn get_log_message(&self, buf: &mut [u8]) {
        unsafe { get_log_message(self.duel_pointer, buf.as_mut_ptr()) };
    }

    pub fn get_message(&self, buf: &mut [u8]) -> i32 {
        unsafe { get_message(self.duel_pointer, buf.as_mut_ptr()) }
    }

    pub fn process(&self) -> ProcessResult {
        let raw = unsafe { process(self.duel_pointer) };
        ProcessResult::from_bytes(raw.to_le_bytes())
    }

    pub fn new_card(&self, code: u32, owner: CorePlayer, playerid: CorePlayer, location: Location, sequence: u8, position: Position) {
        unsafe { new_card(self.duel_pointer, code, owner as u8, playerid as u8, location.bits(), sequence, position as u8) };
    }

    pub fn new_tag_card(&self, code: u32, owner: CorePlayer, location: Location) {
        unsafe { new_tag_card(self.duel_pointer, code, owner as u8, location.bits()) };
    }

    pub fn query_card(&self, player: CorePlayer, location: Location, sequence: u8, query_flag: Query, buf: &mut [u8], use_cache: bool) -> i32 {
        unsafe { query_card(self.duel_pointer, player as u8, location.bits(), sequence, query_flag.bits(), buf.as_mut_ptr(), use_cache as i32) }
    }

    pub fn query_field_count(&self, player: CorePlayer, location: Location) -> i32 {
        unsafe { query_field_count(self.duel_pointer, player as u8, location.bits()) }
    }

    pub fn query_field_card(&self, player: CorePlayer, location: Location, query_flag: Query, buf: &mut [u8], use_cache: bool) -> i32 {
        unsafe { query_field_card(self.duel_pointer, player as u8, location.bits(), query_flag.bits(), buf.as_mut_ptr(), use_cache as i32) }
    }

    pub fn query_field_info(&self, buf: &mut [u8]) -> i32 {
        unsafe { query_field_info(self.duel_pointer, buf.as_mut_ptr()) }
    }

    pub fn set_responsei(&self, value: i32) {
        unsafe { set_responsei(self.duel_pointer, value) };
    }

    pub fn set_responseb(&self, buf: &[u8]) {
        unsafe { set_responseb(self.duel_pointer, buf.as_ptr() as *mut u8) };
    }

    pub fn preload_script(&self, script_name: &str) -> i32 {
        let cpath = CString::new(script_name).unwrap();
        unsafe { preload_script(self.duel_pointer, cpath.as_ptr()) }
    }

    pub fn shuffle_deck(&self, deck: &mut [u32]) {
        self.shuffler.shuffle_deck(deck);
    }
}

impl Drop for Duel {
    fn drop(&mut self) {
        self.end();
    }
}

#[derive(Debug, Clone, Copy, Specifier, PartialEq, Eq)]
#[bits = 4]
pub enum ProcessResultFlags {
    None = 0,
    Waiting = 1,
    End = 2
}

#[bitfield]
pub struct ProcessResult {
    pub data_length: B28,
    pub flags: ProcessResultFlags,
}

#[cfg(test)]
mod tests {
    use super::*;

    // C++ constants from ocgcore/common.h:
    //   #define PROCESSOR_BUFFER_LEN  0x0fffffff
    //   #define PROCESSOR_FLAG        0xf0000000
    //   #define PROCESSOR_NONE        0
    //   #define PROCESSOR_WAITING     0x10000000
    //   #define PROCESSOR_END         0x20000000
    //
    // In C++: engLen = result & PROCESSOR_BUFFER_LEN
    //         engFlag = result & PROCESSOR_FLAG
    // Rust ProcessResult maps B28 to bits 0-27 and ProcessResultFlags to bits 28-31.

    const PROCESSOR_BUFFER_LEN: u32 = 0x0fffffff;
    const PROCESSOR_WAITING: u32 = 0x10000000;
    const PROCESSOR_END: u32 = 0x20000000;

    fn from_u32(raw: u32) -> ProcessResult {
        ProcessResult::from_bytes(raw.to_le_bytes())
    }

    fn to_u32(result: ProcessResult) -> u32 {
        u32::from_le_bytes(result.into_bytes())
    }

    fn cpp_extract_length(raw: u32) -> u32 {
        raw & PROCESSOR_BUFFER_LEN
    }

    fn cpp_extract_flag(raw: u32) -> u32 {
        raw & 0xf0000000
    }

    #[test]
    fn test_bit_layout_roundtrip() {
        let values: &[u32] = &[
            0x00000000,
            0x00000001,
            0x0000002a,
            0x0fffffff,
            0x10000000,
            0x10000005,
            0x20000000,
            0x2000000a,
        ];
        for &original in values {
            let result = from_u32(original);
            let roundtripped = to_u32(result);
            assert_eq!(
                original, roundtripped,
                "roundtrip failed for {original:#010x}: got {roundtripped:#010x}",
            );
        }
    }

    #[test]
    fn test_data_length_matches_cpp_extraction() {
        let values: &[u32] = &[
            0x00000000,
            0x00000005,
            0x0fffffff,
            0x10000000,
            0x1000000a,
            0x20000000,
            0x20000014,
        ];
        for &raw in values {
            let result = from_u32(raw);
            let actual_length = result.data_length();
            let expected_length = cpp_extract_length(raw);
            assert_eq!(
                actual_length, expected_length,
                "data_length mismatch for raw={raw:#010x}: got {actual_length}, expected {expected_length}",
            );
        }
    }

    #[test]
    fn test_flags_match_cpp_extraction() {
        let values: &[u32] = &[
            0x00000000,
            0x00000005,
            0x0fffffff,
            0x10000000,
            0x1000000a,
            0x20000000,
            0x20000014,
        ];
        for &raw in values {
            let result = from_u32(raw);
            let cpp_flag = cpp_extract_flag(raw);
            let rust_flag = result.flags();

            match cpp_flag {
                0 => assert_eq!(
                    rust_flag, ProcessResultFlags::None,
                    "flags mismatch for raw={raw:#010x}: expected None, got {rust_flag:?}",
                ),
                PROCESSOR_WAITING => assert_eq!(
                    rust_flag, ProcessResultFlags::Waiting,
                    "flags mismatch for raw={raw:#010x}: expected Waiting, got {rust_flag:?}",
                ),
                PROCESSOR_END => assert_eq!(
                    rust_flag, ProcessResultFlags::End,
                    "flags mismatch for raw={raw:#010x}: expected End, got {rust_flag:?}",
                ),
                other => panic!("unexpected C++ flag {other:#010x} for raw={raw:#010x}"),
            }
        }
    }

    #[test]
    fn test_flags_eq_cpp_raw_values() {
        // When stored in a ProcessResult with data_length=0, the flags field
        // at bits 28-31 should match C++ PROCESSOR_WAITING / PROCESSOR_END.
        let result_waiting = from_u32(PROCESSOR_WAITING);
        assert_eq!(result_waiting.flags(), ProcessResultFlags::Waiting);
        assert_eq!(result_waiting.data_length(), 0);

        let result_end = from_u32(PROCESSOR_END);
        assert_eq!(result_end.flags(), ProcessResultFlags::End);
        assert_eq!(result_end.data_length(), 0);
    }

    #[test]
    fn test_max_data_length_isolates_from_flags() {
        // C++ PROCESSOR_BUFFER_LEN = 0x0fffffff. Both length and flag
        // should coexist without crosstalk.
        let raw = PROCESSOR_BUFFER_LEN | PROCESSOR_WAITING; // 0x1fffffff
        let result = from_u32(raw);
        assert_eq!(result.data_length(), PROCESSOR_BUFFER_LEN);
        assert_eq!(result.flags(), ProcessResultFlags::Waiting);

        let raw = PROCESSOR_BUFFER_LEN | PROCESSOR_END; // 0x2fffffff
        let result = from_u32(raw);
        assert_eq!(result.data_length(), PROCESSOR_BUFFER_LEN);
        assert_eq!(result.flags(), ProcessResultFlags::End);
    }
}

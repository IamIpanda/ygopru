// ygopro-core-wrapper: FFI bindings for ocgcore
// 对应: ../ygopro/ocgcore/ocgapi.h

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use std::ffi::CString;
use std::os::raw::c_char;
use std::os::raw::c_int;

pub use ygopro_data::data::CoreCard;

pub type intptr_t = isize;

pub const SEED_COUNT: usize = 8;
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

    pub fn shuffle_deck(seeds: *const u32, deck: *mut u32, count: usize);
}

// ==================================================
// Safe wrappers
// ==================================================

pub fn create_duel_safe(seeds: &[u32; SEED_COUNT]) -> intptr_t {
    unsafe { create_duel_v2(seeds.as_ptr()) }
}

pub fn start_duel_with_rule(pduel: intptr_t, duel_options: u16, duel_rule: u8) {
    let options = ((duel_rule as u32) << 16) | (duel_options as u32);
    unsafe { start_duel(pduel, options) }
}

pub fn preload_script_from_path(pduel: intptr_t, path: &str) -> i32 {
    let cpath = CString::new(path).unwrap();
    unsafe { preload_script(pduel, cpath.as_ptr()) }
}

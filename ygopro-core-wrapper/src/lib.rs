
//! FFI bindings for [`ygopro-core`](https://github.com/Fluorohydride/ygopro-core).
//! 
//! This project use `ygopro-core` and `lua` as its submodule, and use `cxx` to compile them.
//!
//! In this doc, `ygopro-core` will be short as `ygocore`.

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]
#![warn(missing_docs)]

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

/// The C-style integer used to hold a duel pointer.
pub type intptr_t = isize;

pub mod constants;
pub mod random;
pub use constants::*;
pub use random::DuelSeed;

use crate::random::SEED_COUNT;
/// Callback to fetch a script by its name.
pub type script_reader = Option<extern "C" fn(*const c_char, *mut c_int) -> *mut u8>;
/// Callback to fetch a card's data by its code.
pub type card_reader = Option<extern "C" fn(u32, *mut CoreCard) -> u32>;
/// Callback invoked when the core emits a message.
pub type message_handler = Option<extern "C" fn(intptr_t, u32) -> u32>;

#[cfg(not(feature = "dual"))]
mod official;
#[cfg(not(feature = "dual"))]
mod koishi;

#[cfg(feature = "dual")]
pub mod official;
#[cfg(feature = "dual")]
pub mod koishi;

mod ygocore;

#[cfg(all(not(feature = "dual"), feature = "koishi"))]
pub use koishi::*;
#[cfg(all(not(feature = "dual"), not(feature = "koishi")))]
pub use official::*;

/// The ygocore implementation a dual build can pick from.
#[cfg(feature = "dual")]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum CoreKind {
    /// The official core.
    #[default]
    Official,
    /// The koishi core.
    Koishi
}

/// A ygocore duel wrapper, which handles the raw FFI bindings.
pub struct Duel {
    duel_pointer: intptr_t,
    shuffler: random::MTRandom,
    lua: mlua::Lua,
    api: ygocore::ygocore,
    /// Whether the duel has finished.
    pub ended: bool,
}

static DUEL_LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());

impl Duel {
    fn new_with_api(seed: DuelSeed, api: ygocore::ygocore) -> Self {
        let _guard = DUEL_LIFECYCLE_LOCK.lock();
        let (duel_pointer, seed_array) = match seed {
            DuelSeed::None => {
                let mut seeds = [0; 8];
                for i in 0..8 {
                    seeds[i] = rand::random();
                }
                (unsafe { (api.create_duel_v2)(seeds.as_ptr()) }, seeds)
            },
            DuelSeed::Single(s) => (unsafe { (api.create_duel)(s) }, [s; 8]),
            DuelSeed::Complicated(seq) => (unsafe { (api.create_duel_v2)(seq.as_ptr()) }, seq),
        };
        let shuffler = random::MTRandom::new(DuelSeed::Complicated(seed_array));
        let lua = unsafe { mlua::Lua::get_or_init_from_ptr((api.lua_state)(duel_pointer)) }.clone();
        Self { duel_pointer, shuffler, lua, api, ended: false }
    }

    /// Create a duel by [`DuelSeed`].
    /// 
    /// This is a wrapper of [`create_duel`] and [`create_duel_v2`].
    #[cfg(not(feature = "dual"))]
    pub fn new(seed: DuelSeed) -> Self {
        Self::new_with_api(seed, api())
    }

    /// Create a duel by [`DuelSeed`] on the given [`CoreKind`].
    #[cfg(feature = "dual")]
    pub fn new(seed: DuelSeed, kind: CoreKind) -> Self {
        let api = match kind {
            CoreKind::Official => official::api(),
            CoreKind::Koishi => koishi::api(),
        };
        Self::new_with_api(seed, api)
    }

    /// Get a handle to the lua state owned by this duel.
    ///
    /// Returns `None` once the duel has ended.
    ///
    /// # Safety
    ///
    /// The lua state dies with the duel: [`Duel::end`] destroys it. Everything derived from the
    /// returned handle - the `Lua` itself, and any `Table`, `Function`, `Thread` or other value
    /// obtained through it - must not outlive the duel. Using or dropping such a value after the
    /// duel has ended is undefined behaviour, and the caller is responsible for guaranteeing that
    /// nothing lua related leaks past [`Duel::end`].
    pub unsafe fn lua(&self) -> Option<mlua::Lua> {
        if self.ended { return None }
        Some(self.lua.clone())
    }

    /// Start the duel with the given [`DuelOptions`] and [`MasterRule`].
    /// 
    /// That is a wrapper of [`start_duel`].
    pub fn start(&self, options: DuelOptions, rule: MasterRule) {
        let opt = ((rule as u32) << 16) | (options.bits() as u32);
        unsafe { (self.api.start_duel)(self.duel_pointer, opt) };
    }

    /// End the duel.
    /// Do nothing if the duel has already ended.
    /// 
    /// That is a wrapper of [`end_duel`].
    pub fn end(&mut self) {
        if self.ended { return }
        let _guard = DUEL_LIFECYCLE_LOCK.lock();
        unsafe { (self.api.end_duel)(self.duel_pointer) };
        self.ended = true;
    }

    /// Set a [`CorePlayer`]'s lp, start hand and draw counts. Only used when duel starts.
    /// 
    /// That is a wrapper of [`set_player_info`].
    pub fn set_player_info(&self, player: CorePlayer, lp: i32, start_count: i32, draw_count: i32) {
        unsafe { (self.api.set_player_info)(self.duel_pointer, player as i32, lp, start_count, draw_count) };
    }

    /// Get logging message sent by ygocore.
    /// 
    /// That often produced by `print()` in lua scripts.
    /// 
    /// That is a wrapper of [`get_log_message`].
    pub fn get_log_message(&self, buf: &mut [u8]) {
        unsafe { (self.api.get_log_message)(self.duel_pointer, buf.as_mut_ptr()) };
    }

    /// Get a [`gm::Message`](ygopro_data::message::gm::Message) sent by ygocore.
    /// 
    /// Returns the message length in bytes.
    /// 
    /// That is a wrapper of [`get_message`].
    pub fn get_message(&self, buf: &mut [u8]) -> i32 {
        unsafe { (self.api.get_message)(self.duel_pointer, buf.as_mut_ptr()) }
    }

    /// Process the current input and evolve the status.
    /// 
    /// Returns the [`ProcessResult`].
    /// 
    /// That is a wrapper of [`process`].
    pub fn process(&self) -> ProcessResult {
        let raw = unsafe { (self.api.process)(self.duel_pointer) };
        ProcessResult::from_bytes(raw.to_le_bytes())
    }

    /// Add a card into the duel, with the given code, owner, controller, location, sequence and position.
    /// 
    /// That is a wrapper of [`new_card`].
    pub fn new_card(&self, code: u32, owner: CorePlayer, playerid: CorePlayer, location: Location, sequence: u8, position: Position) {
        unsafe { (self.api.new_card)(self.duel_pointer, code, owner as u8, playerid as u8, location.bits(), sequence, position.bits()) };
    }

    /// Add a card into the duel's tag deck or tag extra.
    /// 
    /// That is a wrapper of [`new_tag_card`].
    pub fn new_tag_card(&self, code: u32, owner: CorePlayer, location: Location) {
        unsafe { (self.api.new_tag_card)(self.duel_pointer, code, owner as u8, location.bits()) };
    }

    /// Query one card's info, and write the result into the buffer.
    /// 
    /// Returns the result length in bytes, or a negative value on failure.
    /// 
    /// That is a wrapper of [`query_card`].
    pub fn query_card(&self, player: CorePlayer, location: Location, sequence: u8, query_flag: Query, buf: &mut [u8], use_cache: bool) -> i32 {
        unsafe { (self.api.query_card)(self.duel_pointer, player as u8, location.bits(), sequence, query_flag.bits(), buf.as_mut_ptr(), use_cache as i32) }
    }

    /// Query how many cards are in the player's [`Location`].
    /// 
    /// That is a wrapper of [`query_field_count`].
    pub fn query_field_count(&self, player: CorePlayer, location: Location) -> i32 {
        unsafe { (self.api.query_field_count)(self.duel_pointer, player as u8, location.bits()) }
    }

    /// Query all cards in the player's [`Location`], and write the result into the buffer.
    /// 
    /// Returns the total result length in bytes, or a negative value on failure.
    /// 
    /// That is a wrapper of [`query_field_card`].
    pub fn query_field_card(&self, player: CorePlayer, location: Location, query_flag: Query, buf: &mut [u8], use_cache: bool) -> i32 {
        unsafe { (self.api.query_field_card)(self.duel_pointer, player as u8, location.bits(), query_flag.bits(), buf.as_mut_ptr(), use_cache as i32) }
    }

    /// Query the field info, such as lp, counters and chain, and write the result into the buffer.
    /// 
    /// Returns the result length in bytes.
    /// 
    /// That is a wrapper of [`query_field_info`].
    pub fn query_field_info(&self, buf: &mut [u8]) -> i32 {
        unsafe { (self.api.query_field_info)(self.duel_pointer, buf.as_mut_ptr()) }
    }

    /// Set the player's response as an integer.
    /// 
    /// That is a wrapper of [`set_responsei`].
    pub fn set_responsei(&self, value: i32) {
        unsafe { (self.api.set_responsei)(self.duel_pointer, value) };
    }

    /// Set the player's response as a byte buffer.
    /// 
    /// That is a wrapper of [`set_responseb`].
    pub fn set_responseb(&self, buf: &[u8]) {
        unsafe { (self.api.set_responseb)(self.duel_pointer, buf.as_ptr() as *mut u8) };
    }

    /// Preload a lua script into the duel.
    /// 
    /// Returns 0 if the script is loaded successfully.
    /// 
    /// That is a wrapper of [`preload_script`].
    pub fn preload_script(&self, script_name: &str) -> i32 {
        let cpath = CString::new(script_name).unwrap();
        unsafe { (self.api.preload_script)(self.duel_pointer, cpath.as_ptr()) }
    }

    /// Shuffle a deck with the duel's shuffler, so the replay can be reproduced.
    /// 
    /// That is a wrapper of [`shuffle_deck`](random::MTRandom::shuffle_deck).
    pub fn shuffle_deck(&self, deck: &mut [u32]) {
        self.shuffler.shuffle_deck(deck);
    }

    /// Get the seed sequence of the duel's shuffler, used to reproduce the replay.
    /// 
    /// That is a wrapper of [`seed_sequence`](random::MTRandom::seed_sequence).
    pub fn seed(&self) -> &[u32; SEED_COUNT] {
        self.shuffler.seed_sequence()
    }
}

impl Drop for Duel {
    fn drop(&mut self) {
        self.end();
    }
}

/// The state the duel reached after a [`process`] call.
///
/// # Note
/// `Waiting` is not a reliable signal that the duel is halted for input: ygocore
/// may report it even when it still needs to keep processing. To decide whether
/// to stop evolving, inspect the last emitted message instead (as the `evolve`
/// function in the `ygopro` crate does).
///
/// Also, after the first [`Win`](ygopro_data::message::gm::Win) message the core's
/// output becomes unreliable: it keeps resending `Win` and may emit further
/// messages, so the first `Win` should be treated as the end of the duel.
#[derive(Debug, Clone, Copy, Specifier, PartialEq, Eq)]
#[bits = 4]
pub enum ProcessResultFlags {
    /// The duel keeps processing normally.
    None = 0,
    /// The duel is waiting for a player response.
    Waiting = 1,
    /// The duel has finished.
    End = 2
}

/// The result of a [`process`] call.
///
/// `data_length` is the length in bytes of the message data the core has produced,
/// to be read with [`get_message`]. `flags` tells the state the duel reached.
#[bitfield]
pub struct ProcessResult {
    /// Length in bytes of the message data the core has produced.
    pub data_length: B28,
    /// The state the duel reached after processing.
    pub flags: ProcessResultFlags,
}

#[cfg(test)]
mod tests {
    use super::Duel;
    use super::DuelSeed;
    #[cfg(feature = "dual")]
    use super::CoreKind;

    #[test]
    fn lua_is_available_until_end() {
        #[cfg(feature = "dual")]
        let mut duel = Duel::new(DuelSeed::Single(42), CoreKind::Official);
        #[cfg(not(feature = "dual"))]
        let mut duel = Duel::new(DuelSeed::Single(42));
        let lp = {
            let lua = unsafe { duel.lua() }.unwrap();
            let table: mlua::Table = lua.globals().get("Duel").unwrap();
            let get_lp: mlua::Function = table.get("GetLP").unwrap();
            get_lp.call::<i32>(0).unwrap()
        };
        assert_eq!(lp, 8000);
        duel.end();
        assert!(unsafe { duel.lua() }.is_none());
    }
}

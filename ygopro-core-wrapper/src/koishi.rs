//! FFI bindings for [`purerosefallen/ygopro-core`](https://github.com/purerosefallen/ygopro-core).
//!
//! These are the entry points exported by the koishi core. In a `dual` build every
//! symbol of this core is prefixed, which the `link_name` attributes below handle.
//!
//! In this doc, `ygopro-core` will be short as `ygocore`.

use std::os::raw::c_char;

use crate::card_reader;
use crate::intptr_t;
use crate::message_handler;
use crate::script_reader;
use crate::ygocore::ygocore;

unsafe extern "C" {
    /// Set the callback used to load scripts.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_set_script_reader")]
    pub fn set_script_reader(f: script_reader);
    /// Set the callback used to load card data.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_set_card_reader")]
    pub fn set_card_reader(f: card_reader);
    /// Set the callback invoked when the core emits a message.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_set_message_handler")]
    pub fn set_message_handler(f: message_handler);

    /// Create a duel with a single seed.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_create_duel")]
    pub fn create_duel(seed: u32) -> intptr_t;
    /// Create a duel with a seed sequence.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_create_duel_v2")]
    pub fn create_duel_v2(seed_sequence: *const u32) -> intptr_t;
    /// Start the duel with the given options and master rule.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_start_duel")]
    pub fn start_duel(pduel: intptr_t, options: u32);
    /// End the duel.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_end_duel")]
    pub fn end_duel(pduel: intptr_t);
    /// Set a player's lp, start hand and draw counts.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_set_player_info")]
    pub fn set_player_info(pduel: intptr_t, playerid: i32, lp: i32, startcount: i32, drawcount: i32);
    /// Get the lua log message.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_get_log_message")]
    pub fn get_log_message(pduel: intptr_t, buf: *mut u8);
    /// Get the next game message into the buffer, returns its length in bytes.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_get_message")]
    pub fn get_message(pduel: intptr_t, buf: *mut u8) -> i32;
    /// Process the duel and evolve its status.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_process")]
    pub fn process(pduel: intptr_t) -> u32;
    /// Add a card into the duel.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_new_card")]
    pub fn new_card(pduel: intptr_t, code: u32, owner: u8, playerid: u8, location: u8, sequence: u8, position: u8);
    /// Add a card into the duel's tag deck or tag extra.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_new_tag_card")]
    pub fn new_tag_card(pduel: intptr_t, code: u32, owner: u8, location: u8);
    /// Query one card's info and write the result into the buffer.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_query_card")]
    pub fn query_card(pduel: intptr_t, playerid: u8, location: u8, sequence: u8, query_flag: u32, buf: *mut u8, use_cache: i32) -> i32;
    /// Query the card count of a location.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_query_field_count")]
    pub fn query_field_count(pduel: intptr_t, playerid: u8, location: u8) -> i32;
    /// Query all cards of a location and write the result into the buffer.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_query_field_card")]
    pub fn query_field_card(pduel: intptr_t, playerid: u8, location: u8, query_flag: u32, buf: *mut u8, use_cache: i32) -> i32;
    /// Query the field info and write the result into the buffer.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_query_field_info")]
    pub fn query_field_info(pduel: intptr_t, buf: *mut u8) -> i32;
    /// Set the player's response as an integer.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_set_responsei")]
    pub fn set_responsei(pduel: intptr_t, value: i32);
    /// Set the player's response as a byte buffer.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_set_responseb")]
    pub fn set_responseb(pduel: intptr_t, buf: *mut u8);
    /// Preload a lua script into the duel.
    ///
    /// Binding of the original `ygopro-core` C++ function.
    ///
    /// See also: [ygopro-core docs](https://github.com/purerosefallen/ygopro-core).
    #[cfg_attr(feature = "dual", link_name = "koishi_preload_script")]
    pub fn preload_script(pduel: intptr_t, script_name: *const c_char) -> i32;

    /// Get the lua state the core created for the given duel.
    #[cfg_attr(feature = "dual", link_name = "koishi_ygocore_lua_state")]
    pub fn ygocore_lua_state(pduel: intptr_t) -> *mut mlua::lua_State;
}

pub(crate) fn api() -> ygocore {
    ygocore {
        set_script_reader: set_script_reader,
        set_card_reader: set_card_reader,
        set_message_handler: set_message_handler,
        create_duel: create_duel,
        create_duel_v2: create_duel_v2,
        start_duel: start_duel,
        end_duel: end_duel,
        set_player_info: set_player_info,
        get_log_message: get_log_message,
        get_message: get_message,
        process: process,
        new_card: new_card,
        new_tag_card: new_tag_card,
        query_card: query_card,
        query_field_count: query_field_count,
        query_field_card: query_field_card,
        query_field_info: query_field_info,
        set_responsei: set_responsei,
        set_responseb: set_responseb,
        preload_script: preload_script,
        lua_state: ygocore_lua_state,
    }
}

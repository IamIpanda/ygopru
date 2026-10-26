//! The function pointer table of a ygopro-core implementation.

use std::os::raw::c_char;

use crate::card_reader;
use crate::intptr_t;
use crate::message_handler;
use crate::script_reader;

pub(crate) struct ygocore {
    pub(crate) set_script_reader: unsafe extern "C" fn(script_reader),
    pub(crate) set_card_reader: unsafe extern "C" fn(card_reader),
    pub(crate) set_message_handler: unsafe extern "C" fn(message_handler),
    pub(crate) create_duel: unsafe extern "C" fn(u32) -> intptr_t,
    pub(crate) create_duel_v2: unsafe extern "C" fn(*const u32) -> intptr_t,
    pub(crate) start_duel: unsafe extern "C" fn(intptr_t, u32),
    pub(crate) end_duel: unsafe extern "C" fn(intptr_t),
    pub(crate) set_player_info: unsafe extern "C" fn(intptr_t, i32, i32, i32, i32),
    pub(crate) get_log_message: unsafe extern "C" fn(intptr_t, *mut u8),
    pub(crate) get_message: unsafe extern "C" fn(intptr_t, *mut u8) -> i32,
    pub(crate) process: unsafe extern "C" fn(intptr_t) -> u32,
    pub(crate) new_card: unsafe extern "C" fn(intptr_t, u32, u8, u8, u8, u8, u8),
    pub(crate) new_tag_card: unsafe extern "C" fn(intptr_t, u32, u8, u8),
    pub(crate) query_card: unsafe extern "C" fn(intptr_t, u8, u8, u8, u32, *mut u8, i32) -> i32,
    pub(crate) query_field_count: unsafe extern "C" fn(intptr_t, u8, u8) -> i32,
    pub(crate) query_field_card: unsafe extern "C" fn(intptr_t, u8, u8, u32, *mut u8, i32) -> i32,
    pub(crate) query_field_info: unsafe extern "C" fn(intptr_t, *mut u8) -> i32,
    pub(crate) set_responsei: unsafe extern "C" fn(intptr_t, i32),
    pub(crate) set_responseb: unsafe extern "C" fn(intptr_t, *mut u8),
    pub(crate) preload_script: unsafe extern "C" fn(intptr_t, *const c_char) -> i32,
    pub(crate) lua_state: unsafe extern "C" fn(intptr_t) -> *mut mlua::lua_State,
}

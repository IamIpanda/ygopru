#![doc = include_str!("../README.md")]

pub mod single_duel;
pub mod tag_duel;
pub mod duel;
pub mod host;
pub mod managers;
pub mod plugin;
pub mod replay;
pub mod message;
pub mod command;
pub mod cli;
pub mod ffi;
pub mod ygopro_handlers;
pub mod ygocore_handlers;
pub mod player;
mod configuration;
#[cfg(feature = "zip")]
pub mod ypk;

pub use configuration::Configuration as Configuration;
pub use plugin::version_check::PRO_VERSION as PRO_VERSION;
pub use host::DuelHost as DuelHost;

use std::ffi::CStr;
use std::os::raw::c_char;
use managers::*;

#[macro_use] extern crate ygopro_derive;

/// Initialize the global [`managers`] and the ygocore callbacks.
pub fn init() {
    managers::config_manager::init();
    managers::i18n::init();
    #[cfg(feature = "zip")]
    ypk::archive_manager::init();
    managers::data_manager::init();
    managers::deck_manager::init();
    init_core();
}


macro_rules! core_message_handler {
    ($handler:ident, $get_log_message:path) => {{
        extern "C" fn $handler(pduel: isize, message_type: u32) -> u32 {
            let mut buffer = [0u8; 1024];
            unsafe { $get_log_message(pduel, buffer.as_mut_ptr()) };
            let c_message = unsafe { CStr::from_ptr(buffer.as_ptr() as *const c_char) };
            log::debug!("core message[{}]: {}", message_type, c_message.to_string_lossy());
            0
        }
        $handler
    }};
}

/// Set the ygocore callbacks
/// ([`set_script_reader`](ygopro_core_wrapper::set_script_reader),
/// [`set_card_reader`](ygopro_core_wrapper::set_card_reader),
/// [`set_message_handler`](ygopro_core_wrapper::set_message_handler)).
pub fn init_core() {
    #[cfg(feature = "dual")]
    unsafe {
        ygopro_core_wrapper::official::set_script_reader(Some(data_manager::script_reader));
        ygopro_core_wrapper::official::set_card_reader(Some(data_manager::card_reader));
        ygopro_core_wrapper::official::set_message_handler(Some(core_message_handler!(h1, ygopro_core_wrapper::official::get_log_message)));
        ygopro_core_wrapper::koishi::set_script_reader(Some(data_manager::script_reader));
        ygopro_core_wrapper::koishi::set_card_reader(Some(data_manager::card_reader));
        ygopro_core_wrapper::koishi::set_message_handler(Some(core_message_handler!(h2, ygopro_core_wrapper::koishi::get_log_message)));
    }
    #[cfg(not(feature = "dual"))]
    unsafe {
        ygopro_core_wrapper::set_script_reader(Some(data_manager::script_reader));
        ygopro_core_wrapper::set_card_reader(Some(data_manager::card_reader));
        ygopro_core_wrapper::set_message_handler(Some(core_message_handler!(h, ygopro_core_wrapper::get_log_message)));
    }
}

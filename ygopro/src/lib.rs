pub mod single_duel;
pub mod tag_duel;
pub mod common;
pub mod managers;

use std::ffi::CStr;
use std::os::raw::c_char;

use managers::*;
pub const PRO_VERSION: u16 = 0x1362;

pub fn init() {
    init_config();
    init_core();
    single_duel::init();
}

pub fn init_config() {
    let config_path = std::env::var("YGOPRO_CONFIG_PATH").unwrap_or_else(|_| "system.conf".to_string());

    let mut config_manager = ConfigManager::new();
    config_manager.load(&config_path).ok();
    let db_path = config_manager.get_or("db_path", "cards.cdb").to_string();
    managers::config_manager::set_global(config_manager);

    let mut data_manager = DataManager::new();
    data_manager.load_db(&db_path).expect("Failed to load database");
    managers::data_manager::set_global(data_manager);

    let mut deck_manager = DeckManager::new();
    deck_manager.load_lflist().expect("Failed to load lflist");
    managers::deck_manager::set_global(deck_manager);

    let strings = ygopro_data::data::load_strings_conf("strings.conf");
    managers::i18n::set_strings(strings);
}

pub fn init_core() {
    unsafe {
        ygopro_core_wrapper::set_script_reader(Some(data_manager::script_reader));
        ygopro_core_wrapper::set_card_reader(Some(data_manager::card_reader));
        ygopro_core_wrapper::set_message_handler(Some(core_message_handler));
    }
}

extern "C" fn core_message_handler(pduel: isize, message_type: u32) -> u32 {
    let mut buffer = [0u8; 1024];
    unsafe { ygopro_core_wrapper::get_log_message(pduel, buffer.as_mut_ptr()); }
    let c_message = unsafe { CStr::from_ptr(buffer.as_ptr() as *const c_char) };
    log::debug!("core message[{}]: {}", message_type, c_message.to_string_lossy());
    0
}

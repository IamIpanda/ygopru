pub mod single_duel;
pub mod tag_duel;
pub mod common;
pub mod managers;

use managers::*;
pub const PRO_VERSION: u16 = 0x1362;

pub fn init() {
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

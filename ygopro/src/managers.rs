pub mod data_manager {
    use std::ffi::c_char;
    use std::ffi::c_int;
    use std::ffi::CStr;
    use std::fs;
    use std::sync::Arc;

    use arc_swap::ArcSwapOption;
    use hashbrown::HashMap;
    use parking_lot::Mutex;

    use ygopro_data::constants::Type;
    use ygopro_data::data::Card;
    use ygopro_data::data::CoreCard;

    static SCRIPT_BUFFER: Mutex<[u8; 0x100000]> = Mutex::new([0u8; 0x100000]);
    static GLOBAL_DATA_MANAGER: ArcSwapOption<DataManager> = ArcSwapOption::const_empty();

    pub fn set_global(data_manager: DataManager) {
        GLOBAL_DATA_MANAGER.store(Some(Arc::new(data_manager)));
    }

    pub fn load() -> arc_swap::Guard<Option<Arc<DataManager>>> {
        GLOBAL_DATA_MANAGER.load()
    }

    const CARD_ARTWORK_VERSIONS_OFFSET: u32 = 20;

    fn is_alternative(code: u32, alias: u32) -> bool {
        alias != 0 && alias < code + CARD_ARTWORK_VERSIONS_OFFSET && code < alias + CARD_ARTWORK_VERSIONS_OFFSET
    }

    pub struct DataManager {
        pub cards: HashMap<u32, Card>,
        extra_setcode: HashMap<u32, Vec<u16>>,
    }

    impl DataManager {
        pub fn new() -> Self {
            let mut extra_setcode = HashMap::new();
            extra_setcode.insert(8512558u32, vec![0x8f, 0x54, 0x59, 0x82, 0x13a]);
            extra_setcode.insert(55088578u32, vec![0x8f, 0x54, 0x59, 0x82, 0x13a]);
            Self {
                cards: HashMap::new(),
                extra_setcode,
            }
        }

        pub fn load_db(&mut self, file: &str) -> Result<(), String> {
            let cards = ygopro_data::data::load_db_from_file::<Card>(file)
                .map_err(|e| format!("Failed to load {}: {}", file, e))?;

            for mut card in cards {
                if card.code == 5405695 {
                    card.rule_code = card.alias;
                    card.alias = 0;
                } else if card.alias != 0
                    && !card.card_type.contains(Type::Token)
                    && !is_alternative(card.code, card.alias)
                {
                    card.rule_code = card.alias;
                    card.alias = 0;
                }
                self.cards.insert(card.code, card);
            }

            let pending: Vec<(u32, u32)> = self
                .cards
                .iter()
                .filter_map(|(&code, card)| {
                    if card.rule_code != 0 || card.alias == 0 || card.card_type.contains(Type::Token) {
                        return None;
                    }
                    Some((code, card.alias))
                })
                .collect();

            for (code, alias) in pending {
                let rule_code = self.cards.get(&alias).map(|c| c.rule_code).unwrap_or(0);
                if let Some(card) = self.cards.get_mut(&code) {
                    card.rule_code = rule_code;
                }
            }

            for (code, list) in &self.extra_setcode {
                if list.is_empty() || list.len() > 16 { continue; }
                if let Some(card) = self.cards.get_mut(code) {
                    for (i, &sc) in list.iter().enumerate() {
                        card.setcode[i] = sc;
                    }
                }
            }

            Ok(())
        }

        pub fn get_card(&self, code: u32) -> Option<&Card> {
            self.cards.get(&code)
        }

        pub fn get_core_card(&self, code: u32) -> Option<&CoreCard> {
            self.cards.get(&code).map(|c| &c.card)
        }
    }

    /// Corresponds to `CardReader` in data_manager.cpp:510-514.
    pub extern "C" fn card_reader(code: u32, data: *mut CoreCard) -> u32 {
        if data.is_null() {
            return 0;
        }
        let guard = GLOBAL_DATA_MANAGER.load();
        if let Some(data_manager) = (*guard).as_ref() {
            if let Some(card) = data_manager.get_core_card(code) {
                unsafe { *data = card.clone(); }
                return 0;
            }
        }
        unsafe { *data = CoreCard::default(); }
        0
    }

    /// Corresponds to `ScriptReaderEx` in data_manager.cpp:515-553.
    pub extern "C" fn script_reader(script_path: *const c_char, slen: *mut c_int) -> *mut u8 {
        if script_path.is_null() || slen.is_null() {
            return std::ptr::null_mut();
        }
        let path = unsafe { CStr::from_ptr(script_path).to_string_lossy() };
        let mut buffer = SCRIPT_BUFFER.lock();

        let mut read_file = |file_path: &str| -> Option<usize> {
            fs::read(file_path).ok().and_then(|data| {
                if data.len() >= buffer.len() {
                    return None;
                }
                buffer[..data.len()].copy_from_slice(&data);
                Some(data.len())
            })
        };

        if path.starts_with("./script") {
            let filename = &path[9..];
            if let Some(len) = read_file(&format!("./specials/{}", filename)) {
                unsafe { *slen = len as c_int; }
                return buffer.as_mut_ptr();
            }
            if let Some(len) = read_file(&format!("./expansions/{}", &path[2..])) {
                unsafe { *slen = len as c_int; }
                return buffer.as_mut_ptr();
            }
            if let Some(len) = read_file(path.as_ref()) {
                unsafe { *slen = len as c_int; }
                return buffer.as_mut_ptr();
            }
        } else {
            if let Some(len) = read_file(path.as_ref()) {
                unsafe { *slen = len as c_int; }
                return buffer.as_mut_ptr();
            }
        }

        std::ptr::null_mut()
    }
}

pub mod i18n {
    use std::collections::HashMap;
    use std::sync::Arc;

    use arc_swap::ArcSwapOption;

    static GLOBAL_STRINGS: ArcSwapOption<HashMap<String, HashMap<i32, String>>> = ArcSwapOption::const_empty();

    pub fn set_strings(strings: HashMap<String, HashMap<i32, String>>) {
        GLOBAL_STRINGS.store(Some(Arc::new(strings)));
    }
}

pub mod deck_manager {
    use std::collections::HashMap;
    use std::fs;
    use std::io;
    use std::sync::Arc;

    use arc_swap::ArcSwapOption;

    use ygopro_data::data::parse_lflist_content;
    use ygopro_data::data::LFList;

    static GLOBAL_DECK_MANAGER: ArcSwapOption<DeckManager> = ArcSwapOption::const_empty();

    pub fn set_global(deck_manager: DeckManager) {
        GLOBAL_DECK_MANAGER.store(Some(Arc::new(deck_manager)));
    }

    pub fn load() -> arc_swap::Guard<Option<Arc<DeckManager>>> {
        GLOBAL_DECK_MANAGER.load()
    }

    pub struct DeckManager {
        pub lflists: Vec<LFList>,
    }

    impl DeckManager {
        pub fn new() -> Self {
            Self {
                lflists: Vec::new(),
            }
        }

        pub fn load_lflist(&mut self) -> io::Result<()> {
            self.lflists.clear();
            for path in &["expansions/lflist.conf", "lflist.conf"] {
                if let Ok(content) = fs::read_to_string(path) {
                    self.lflists.extend(parse_lflist_content(&content));
                }
            }
            if !self.lflists.is_empty() {
                self.lflists.push(LFList { hash: 0, name: "N/A".to_string(), content: HashMap::new() });
            }
            Ok(())
        }

        pub fn get_lflist(&self, index: u32) -> Option<&LFList> {
            self.lflists.get(index as usize)
        }

        pub fn get_lflist_name(&self, index: u32) -> &str {
            self.get_lflist(index)
                .map(|l| l.name.as_str())
                .unwrap_or("???")
        }
    }
}

pub mod config_manager {
    use std::fs;
    use std::io;
    use std::sync::Arc;

    use arc_swap::ArcSwapOption;
    use hashbrown::HashMap;

    static GLOBAL_CONFIG_MANAGER: ArcSwapOption<ConfigManager> = ArcSwapOption::const_empty();

    pub fn set_global(config_manager: ConfigManager) {
        GLOBAL_CONFIG_MANAGER.store(Some(Arc::new(config_manager)));
    }

    pub struct ConfigManager {
        entries: HashMap<String, String>,
    }

    impl ConfigManager {
        pub fn new() -> Self {
            Self {
                entries: HashMap::new(),
            }
        }

        pub fn load(&mut self, path: &str) -> io::Result<()> {
            let content = fs::read_to_string(path)?;
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(eq) = line.find('=') {
                    let key = line[..eq].trim().to_string();
                    let value = line[eq + 1..].trim().to_string();
                    self.entries.insert(key, value);
                }
            }
            Ok(())
        }

        pub fn get(&self, key: &str) -> Option<&str> {
            self.entries.get(key).map(|s| s.as_str())
        }

        pub fn get_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
            self.entries
                .get(key)
                .map(|s| s.as_str())
                .unwrap_or(default)
        }
    }
}

pub use data_manager::DataManager;
pub use deck_manager::DeckManager;
pub use config_manager::ConfigManager;
pub use i18n::set_strings;

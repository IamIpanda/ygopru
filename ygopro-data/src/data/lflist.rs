use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::str::FromStr;
use std::path::PathBuf;
use once_cell::sync::OnceCell;

pub struct LFList {
    name: String,
    limits: HashMap<u32, u8>
}

pub static LFLISTS: OnceCell<Vec<LFList>> = OnceCell::new();

impl LFList {
    pub fn new(name: String) -> Self {
        Self {
            name,
            limits: HashMap::new()
        }
    } 

    fn from_file(path: PathBuf) -> std::io::Result<Vec<Self>> {
        let mut file = File::open(path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        Ok(Self::from_string(&buf))
    }

    fn from_string(str: &str) -> Vec<Self> {
        const BIG_BOOM_LFLIST_NAME: &'static str = "__BIG_BOOM__";
        let mut loaded_lflists = Vec::new();
        let mut current_lflist = LFList::new(BIG_BOOM_LFLIST_NAME.to_string());
        for line in str.split("\n") {
            if line.starts_with("#") {
                continue;
            } else if line.starts_with("!") {
                if current_lflist.name != BIG_BOOM_LFLIST_NAME {
                    loaded_lflists.push(current_lflist);
                }
                current_lflist = LFList::new(line[1..].to_string());
            } else {
                let parts = line.split(" ").collect::<Vec<&str>>();
                if parts.len() < 2 { continue; }
                let card_id = u32::from_str(parts[0]);
                let limit = u8::from_str(parts[1]);
                if let (Ok(card_id), Ok(limit)) = (card_id, limit) {
                    current_lflist.limits.insert(card_id, limit);
                }
            }
        }
        loaded_lflists
    }

    pub fn init() -> Vec<Self> {
        todo!();
    }

    pub fn first_tcg() -> i32 {
        for (index, lflist) in LFLISTS.get_or_init(LFList::init).iter().enumerate() {
            if lflist.name.ends_with("TCG") {
                return index as i32;
            }
        }
        return -1;
    } 
}

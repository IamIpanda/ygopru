use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct LFList {
    pub hash: u32,
    pub name: String,
    pub content: HashMap<u32, u8>,
}

impl LFList {
    pub fn new(name: String, content: HashMap<u32, u8>) -> Self {
        let hash = {
            let mut h: u32 = 5381;
            for &byte in name.as_bytes() {
                h = h.wrapping_mul(33).wrapping_add(byte as u32);
            }
            h
        };
        Self { hash, name, content }
    }
}

pub fn load_lflist_single(path: &Path) -> io::Result<Vec<LFList>> {
    let content = fs::read_to_string(path)?;
    Ok(parse_lflist_content(&content))
}

pub fn load_lflist_all() -> io::Result<Vec<LFList>> {
    let lflist_path = Path::new("lflist.conf");
    if !lflist_path.exists() {
        let expansion_dir = Path::new("expansions");
        let mut lists = Vec::new();
        if expansion_dir.is_dir() {
            for entry in fs::read_dir(expansion_dir)? {
                let path = entry?.path();
                if path.extension().map_or(false, |e| e == "conf") {
                    if let Ok(l) = load_lflist_single(&path) { lists.extend(l); }
                }
            }
        }
        if lists.is_empty() && lflist_path.exists() {
            return load_lflist_single(lflist_path);
        }
        return Ok(lists);
    }
    load_lflist_single(lflist_path)
}

fn parse_lflist_content(content: &str) -> Vec<LFList> {
    let mut lists = Vec::new();
    let mut name = String::new();
    let mut limits = HashMap::new();
    for line in content.lines().map(|l| l.trim()) {
        if line.is_empty() || line.starts_with('#') { continue; }
        if line.starts_with('!') {
            if !name.is_empty() { lists.push(LFList::new(std::mem::take(&mut name), std::mem::take(&mut limits))); }
            name = line[1..].to_string();
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, |c| c == ' ' || c == '\t').collect();
        if parts.len() >= 2 {
            if let (Ok(id), Ok(limit)) = (u32::from_str_radix(parts[0].trim(), 10), parts[1].trim().parse::<u8>()) {
                if limit <= 3 { limits.insert(id, limit); }
            }
        }
    }
    if !name.is_empty() { lists.push(LFList::new(name, limits)); }
    if !lists.is_empty() { lists.push(LFList::new("N/A".to_string(), HashMap::new())); }
    lists
}

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LFList {
    pub hash: u32,
    pub name: String,
    pub content: HashMap<u32, u8>,
}

impl LFList {
    pub fn new(name: String, content: HashMap<u32, u8>) -> Self {
        let mut hash: u32 = 0x7dfcee6a;
        for (&code, &count) in &content {
            hash ^= code.rotate_left(18) ^ code.rotate_left(27 + count as u32);
        }
        Self { hash, name, content }
    }
}

pub fn parse_lflist_content(content: &str) -> Vec<LFList> {
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
                if limit <= 2 { limits.insert(id, limit); }
            }
        }
    }
    if !name.is_empty() { lists.push(LFList::new(name, limits)); }
    lists
}

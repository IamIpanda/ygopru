//! The forbidden/limited list (limit list).
//!
//! Provides [`LFList`], which holds the card restrictions and computes the list hash.

use std::collections::HashMap;

const HASH_INITIAL_VALUE: u32 = 0x7dfcee6a;
const CREDIT_MARKER: u32 = 0x43524544;

#[derive(Debug, Clone)]
pub struct LFList {
    pub hash: u32,
    pub name: String,
    pub content: HashMap<u32, u8>,
    pub credit_limits: HashMap<String, u32>,
    pub credits: HashMap<u32, HashMap<String, u32>>,
}

impl LFList {
    pub fn new(name: String) -> Self {
        Self {
            hash: HASH_INITIAL_VALUE,
            name,
            content: HashMap::new(),
            credit_limits: HashMap::new(),
            credits: HashMap::new(),
        }
    }

    pub fn from(name: String, content: HashMap<u32, u8>) -> Self {
        let mut v = Self {
            hash: HASH_INITIAL_VALUE,
            name,
            content,
            credit_limits: HashMap::new(),
            credits: HashMap::new(),
        };
        v.calculate_hash();
        v
    }

    pub fn from_genesys(name: String, content: HashMap<u32, u8>, genesys: u32, glist: HashMap<u32, u32>) -> Self {
        let mut v = Self {
            hash: HASH_INITIAL_VALUE,
            name,
            content,
            credit_limits: HashMap::new(),
            credits: HashMap::new(),
        };
        if genesys > 0 {
            v.credit_limits.insert("genesys".to_string(), genesys);
        }
        for (code, credit) in glist {
            v.credits.entry(code).or_default().insert("genesys".to_string(), credit);
        }
        v.calculate_hash();
        v
    }

    pub fn calculate_hash(&mut self) {
        let mut hash = HASH_INITIAL_VALUE;
        for (&code, &count) in &self.content {
            hash ^= ((code << 18) | (code >> 14)) ^ ((code << (27 + count)) | (code >> (5 - count)));
        }
        for (key, &credit_limit) in &self.credit_limits {
            update_credit_hash(&mut hash, credit_hash(key), credit_limit, CREDIT_MARKER);
        }
        for (&code, credits) in &self.credits {
            for (key, &credit) in credits {
                update_credit_hash(&mut hash, code, credit_hash(key), credit);
            }
        }
        self.hash = hash;
    }

    pub fn credit(&self, code: u32, key: &str) -> u32 {
        self.credits.get(&code).and_then(|credits| credits.get(key)).copied().unwrap_or(0)
    }

    fn push_card(&mut self, code: u32, count: u8) {
        self.content.insert(code, count);
        self.hash ^= ((code << 18) | (code >> 14)) ^ ((code << (27 + count)) | (code >> (5 - count)));
    }

    fn push_credit_limit(&mut self, key: &str, credit_limit: u32) {
        self.credit_limits.insert(key.to_string(), credit_limit);
        update_credit_hash(&mut self.hash, credit_hash(key), credit_limit, CREDIT_MARKER);
    }

    fn push_credit(&mut self, code: u32, key: &str, credit: u32) {
        self.credits.entry(code).or_default().insert(key.to_string(), credit);
        update_credit_hash(&mut self.hash, code, credit_hash(key), credit);
    }
}

fn credit_hash(key: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for byte in key.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

fn update_credit_hash(hash: &mut u32, first: u32, second: u32, third: u32) {
    *hash ^= ((first << 18) | (first >> 14)) ^ ((second << 9) | (second >> 23)) ^ ((third << 27) | (third >> 5));
}

pub fn parse_lflist_content(content: &str) -> Vec<LFList> {
    let mut lists = Vec::new();
    for line in content.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('!') {
            lists.push(LFList::new(name.to_string()));
            continue;
        }
        let Some(list) = lists.last_mut() else { continue };
        let mut tokens = line.split_whitespace();
        let Some(first) = tokens.next() else { continue };
        if let Some(key) = first.strip_prefix('$') {
            let key = if key.is_empty() { tokens.next() } else { Some(key) };
            let Some(key) = key else { continue };
            let Some(credit_limit) = tokens.next().and_then(|value| value.parse::<u32>().ok()) else { continue };
            list.push_credit_limit(key, credit_limit);
            continue;
        }
        let Ok(code) = first.parse::<u32>() else { continue };
        let Some(second) = tokens.next() else { continue };
        if let Some(key) = second.strip_prefix('$') {
            let key = if key.is_empty() { tokens.next() } else { Some(key) };
            let Some(key) = key else { continue };
            let Some(credit) = tokens.next().and_then(|value| value.parse::<u32>().ok()) else { continue };
            list.push_credit(code, key, credit);
            continue;
        }
        let Ok(count) = second.parse::<u8>() else { continue };
        if count <= 2 {
            list.push_card(code, count);
        }
    }
    lists
}

#[cfg(test)]
mod tests {
    use crate::data::lflist::credit_hash;
    use crate::data::lflist::parse_lflist_content;

    #[test]
    fn credit_hash_is_fnv1a() {
        assert_eq!(credit_hash("genesys"), 0x965f7da9);
    }

    /// Reference C++ code used to compute the expected hash:
    ///
    /// ```
    /// #include <cstdint>
    /// #include <cstdio>
    /// #include <unordered_map>
    ///
    /// int main() {
    ///     const uint32_t HASH_INITIAL_VALUE = 0x7dfcee6a;
    ///     std::unordered_map<uint32_t, uint8_t> content = {
    ///         {20292186, 0},
    ///         {91869203, 0},
    ///         {8633261, 1},
    ///         {79606837, 1},
    ///         {23434538, 1},
    ///         {72270339, 2},
    ///     };
    ///     uint32_t hash = HASH_INITIAL_VALUE;
    ///     for (auto& [code, count] : content) {
    ///         hash = hash ^ ((code << 18) | (code >> 14)) ^ ((code << (27 + count)) | (code >> (5 - count)));
    ///     }
    ///     std::printf("hash = 0x%08x\n", hash);
    /// }
    /// ```
    #[test]
    fn calculate_hash_matches_cpp_reference() {
        let content = "
!test
20292186 0
91869203 0
8633261 1
79606837 1
23434538 1
72270339 2
";
        let lists = parse_lflist_content(content);
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].hash, 0xdee2c97b);
    }

    /// Reference C++ code used to compute the expected hash, taken from
    /// `purerosefallen/ygopro` `gframe/deck_manager.cpp`,
    /// `DeckManager::LoadLFListFromLineProvider`:
    ///
    /// ```
    /// #include <cstdint>
    /// #include <cstdio>
    /// #include <string>
    /// #include <unordered_map>
    ///
    /// int main() {
    ///     const uint32_t HASH_INITIAL_VALUE = 0x7dfcee6a;
    ///     const uint32_t CREDIT_MARKER = 0x43524544;
    ///     std::unordered_map<std::string, uint32_t> credit_limits = {
    ///         {"genesys", 100},
    ///     };
    ///     std::unordered_map<uint32_t, std::unordered_map<std::string, uint32_t>> credits = {
    ///         {123, {{"genesys", 50}}},
    ///         {456, {{"genesys", 5}}},
    ///     };
    ///     std::unordered_map<uint32_t, uint8_t> content = {
    ///         {20292186, 0},
    ///         {8633261, 1},
    ///     };
    ///     uint32_t hash = HASH_INITIAL_VALUE;
    ///     for (auto& [name, limit] : credit_limits) {
    ///         uint32_t key = 2166136261u;
    ///         for (auto ch : name) {
    ///             key = key ^ (uint8_t)ch;
    ///             key = key * 16777619u;
    ///         }
    ///         hash = hash ^ ((key << 18) | (key >> 14)) ^ ((limit << 9) | (limit >> 23)) ^ ((CREDIT_MARKER << 27) | (CREDIT_MARKER >> 5));
    ///     }
    ///     for (auto& [code, credit_map] : credits) {
    ///         for (auto& [name, credit] : credit_map) {
    ///             uint32_t key = 2166136261u;
    ///             for (auto ch : name) {
    ///                 key = key ^ (uint8_t)ch;
    ///                 key = key * 16777619u;
    ///             }
    ///             hash = hash ^ ((code << 18) | (code >> 14)) ^ ((key << 9) | (key >> 23)) ^ ((credit << 27) | (credit >> 5));
    ///         }
    ///     }
    ///     for (auto& [code, count] : content) {
    ///         hash = hash ^ ((code << 18) | (code >> 14)) ^ ((code << (27 + count)) | (code >> (5 - count)));
    ///     }
    ///     std::printf("hash = 0x%08x\n", hash);
    /// }
    /// ```
    #[test]
    fn credit_hash_matches_koishi_cpp_reference() {
        let content = "
!genesys
$genesys 100
20292186 0
123 $genesys 50
456 $genesys 5
8633261 1
";
        let lists = parse_lflist_content(content);
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].hash, 0x70517d4c);
        assert_eq!(lists[0].credit_limits["genesys"], 100);
        assert_eq!(lists[0].credits[&123]["genesys"], 50);
    }
}

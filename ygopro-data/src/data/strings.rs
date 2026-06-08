use std::collections::HashMap;
use std::fs;

pub fn load_strings_conf(path: &str) -> HashMap<String, HashMap<i32, String>> {
    let mut map: HashMap<String, HashMap<i32, String>> = HashMap::new();
    let Ok(content) = fs::read_to_string(path) else { return map; };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || !line.starts_with('!') {
            continue;
        }
        let rest = &line[1..];
        let mut parts = rest.splitn(3, ' ');
        let Some(category) = parts.next() else { continue };
        let Some(id_str) = parts.next() else { continue };
        let Ok(id) = id_str.parse::<i32>() else { continue };
        let value = parts.next().unwrap_or("");

        map.entry(category.to_string())
            .or_default()
            .insert(id, value.to_string());
    }

    map
}

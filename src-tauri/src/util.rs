use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use uuid::Uuid;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn canonical_json(value: &Value) -> String {
    sort_json(value).to_string()
}

fn sort_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut ordered = Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(child) = map.get(key) {
                    ordered.insert(key.clone(), sort_json(child));
                }
            }
            Value::Object(ordered)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_json).collect()),
        other => other.clone(),
    }
}

pub fn stable_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn title_from_goal(goal: &str) -> String {
    let line = goal.lines().find(|line| !line.trim().is_empty()).unwrap_or("New chat");
    let trimmed = line.trim();
    let mut chars = trimmed.chars();
    let short: String = chars.by_ref().take(42).collect();
    if chars.next().is_some() {
        format!("{short}…")
    } else if short.is_empty() {
        "New chat".into()
    } else {
        short
    }
}

pub fn workspace_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

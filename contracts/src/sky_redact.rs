use serde_json::Value;

pub fn redact_payload(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_secret_key(key) {
                    if child.is_string() {
                        *child = Value::String("[REDACTED]".to_string());
                    } else {
                        redact_payload(child);
                    }
                } else {
                    redact_payload(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_payload(item);
            }
        }
        _ => {}
    }
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "key", "token", "secret", "password", "auth", "bearer", "api_key",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

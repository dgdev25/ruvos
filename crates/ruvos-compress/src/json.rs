use crate::defaults::{
    JSON_SCORE_ENDPOINT, JSON_SCORE_ERROR, JSON_SCORE_ID_NAME, JSON_SCORE_STATUS, JSON_SCORE_WARN,
};
use crate::defaults::{LONG_STRING_EDGE_CHARS, LONG_STRING_THRESHOLD};
use serde_json::Value;

fn item_score(item: &Value) -> usize {
    let text = item.to_string().to_ascii_lowercase();
    let mut score = 0usize;
    for marker in [
        "error",
        "failed",
        "exception",
        "panic",
        "traceback",
        "fatal",
        "warning",
        "warn",
        "null",
        "denied",
        "timeout",
    ] {
        if text.contains(marker) {
            score += 10;
        }
    }
    if text.contains("id") {
        score += JSON_SCORE_ID_NAME;
    }
    if text.contains("name") {
        score += JSON_SCORE_ID_NAME;
    }
    if let Value::Object(map) = item {
        for key in map.keys() {
            let key = key.to_ascii_lowercase();
            score += match key.as_str() {
                "id" | "name" => JSON_SCORE_ID_NAME,
                "endpoint" | "route" | "path" | "url" | "uri" => JSON_SCORE_ENDPOINT,
                "status" | "status_code" | "code" => JSON_SCORE_STATUS,
                "error" | "message" | "stack" | "trace" | "traceback" => JSON_SCORE_ERROR,
                _ if key.contains("error") => JSON_SCORE_ERROR,
                _ if key.contains("warn") => JSON_SCORE_WARN,
                _ => 0,
            };
        }
    }
    score
}

fn compress_value(value: Value, max_items: usize) -> Value {
    match value {
        Value::Array(items) => {
            if items.len() <= max_items {
                return Value::Array(
                    items
                        .into_iter()
                        .map(|item| compress_value(item, max_items))
                        .collect(),
                );
            }

            let mut keep_indices: Vec<usize> = vec![0, items.len() - 1];
            let mut scored: Vec<(usize, usize)> = items
                .iter()
                .enumerate()
                .map(|(idx, item)| (idx, item_score(item)))
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            for (idx, score) in scored {
                if score == 0 {
                    continue;
                }
                if !keep_indices.contains(&idx) {
                    keep_indices.push(idx);
                }
                if keep_indices.len() >= max_items {
                    break;
                }
            }
            if keep_indices.len() < max_items {
                let step = ((items.len() as f64) / (max_items as f64 - keep_indices.len() as f64))
                    .ceil() as usize;
                let mut idx = 1usize;
                while idx + 1 < items.len() && keep_indices.len() < max_items {
                    if !keep_indices.contains(&idx) {
                        keep_indices.push(idx);
                    }
                    idx = idx.saturating_add(step.max(1));
                }
            }
            keep_indices.sort_unstable();
            keep_indices.dedup();

            let kept: Vec<Value> = keep_indices
                .into_iter()
                .map(|idx| compress_value(items[idx].clone(), max_items))
                .collect();
            Value::Array(kept)
        }
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                out.insert(key, compress_value(value, max_items));
            }
            Value::Object(out)
        }
        Value::String(text) => {
            if text.len() <= LONG_STRING_THRESHOLD {
                Value::String(text)
            } else {
                let head_end = text
                    .char_indices()
                    .nth(LONG_STRING_EDGE_CHARS)
                    .map(|(idx, _)| idx)
                    .unwrap_or(text.len());
                let tail_start = text
                    .char_indices()
                    .rev()
                    .nth(LONG_STRING_EDGE_CHARS)
                    .map(|(idx, _)| idx)
                    .unwrap_or(0);
                let prefix = &text[..head_end];
                let suffix = &text[tail_start..];
                Value::String(format!("{prefix}\n...[compressed]...\n{suffix}"))
            }
        }
        other => other,
    }
}

pub fn compress_json(content: &str, max_items: usize) -> Option<String> {
    let parsed: Value = serde_json::from_str(content).ok()?;
    let compressed = compress_value(parsed, max_items);
    serde_json::to_string_pretty(&compressed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_error_items_and_endpoints() {
        let input = serde_json::json!([
            {"id": 1, "status": "ok"},
            {"id": 2, "status": "ok"},
            {"id": 3, "status": "failed", "error": "timeout"},
            {"id": 4, "status": "ok"},
            {"id": 5, "status": "ok"},
            {"id": 6, "status": "ok"},
            {"id": 7, "status": "ok"},
            {"id": 8, "status": "ok"},
            {"id": 9, "status": "ok"},
            {"id": 10, "status": "ok"},
            {"id": 11, "status": "ok"},
            {"id": 12, "status": "ok"},
            {"id": 13, "status": "ok"},
            {"id": 14, "status": "ok"},
            {"id": 15, "status": "ok"}
        ]);

        let compressed = compress_json(&input.to_string(), 6).expect("compress");
        let out: Value = serde_json::from_str(&compressed).expect("json");
        let arr = out.as_array().expect("array");
        assert_eq!(arr.first().unwrap()["id"], 1);
        assert_eq!(arr.last().unwrap()["id"], 15);
        assert!(arr.iter().any(|item| item.to_string().contains("failed")));
    }

    #[test]
    fn keeps_endpoint_and_status_fields_from_sparse_json_arrays() {
        let input = serde_json::json!([
            {"id": 1, "path": "/health", "status_code": 200},
            {"id": 2, "path": "/users", "status_code": 200},
            {"id": 3, "path": "/payments", "status_code": 500, "error": "timeout"},
            {"id": 4, "path": "/settings", "status_code": 200},
            {"id": 5, "path": "/profile", "status_code": 200},
            {"id": 6, "path": "/billing", "status_code": 200},
            {"id": 7, "path": "/reports", "status_code": 200},
            {"id": 8, "path": "/logs", "status_code": 200},
            {"id": 9, "path": "/search", "status_code": 200},
            {"id": 10, "path": "/admin", "status_code": 200},
            {"id": 11, "path": "/sessions", "status_code": 200},
            {"id": 12, "path": "/audit", "status_code": 200}
        ]);

        let compressed = compress_json(&input.to_string(), 5).expect("compress");
        let out: Value = serde_json::from_str(&compressed).expect("json");
        let arr = out.as_array().expect("array");
        assert!(arr.iter().any(|item| item["path"] == "/payments"));
        assert!(arr.iter().any(|item| item["status_code"] == 500));
    }
}

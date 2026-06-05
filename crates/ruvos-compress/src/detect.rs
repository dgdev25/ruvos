use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Json,
    Code,
    Log,
    Text,
}

pub fn detect_content_type(content: &str) -> ContentKind {
    let trimmed = content.trim_start();
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return ContentKind::Json;
    }

    let lower = content.to_ascii_lowercase();
    let has_log_markers = [
        "error",
        "failed",
        "exception",
        "panic",
        "traceback",
        "fatal",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if has_log_markers {
        return ContentKind::Log;
    }

    let has_code_markers = [
        "fn ", "def ", "class ", "impl ", "struct ", "pub ", "use ", "import ", "return ",
    ]
    .iter()
    .any(|marker| content.contains(marker));
    if has_code_markers {
        return ContentKind::Code;
    }

    ContentKind::Text
}

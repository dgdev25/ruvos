//! Shared compression defaults.

/// Minimum input size before compression runs.
pub const MIN_BYTES: usize = 256;

/// Number of lines preserved from the head of text/log content.
pub const KEEP_HEAD_LINES: usize = 8;

/// Number of lines preserved from the tail of text/log content.
pub const KEEP_TAIL_LINES: usize = 8;

/// Maximum JSON array items to keep when thinning large arrays.
pub const MAX_ARRAY_ITEMS: usize = 12;

/// Maximum retained lines for code-like payloads.
pub const MAX_CODE_LINES: usize = 48;

/// Maximum retained lines for log payloads.
pub const MAX_LOG_LINES: usize = 64;

/// Maximum retained lines for plain text payloads.
pub const MAX_TEXT_LINES: usize = 72;

/// Long-string compression threshold in bytes.
pub const LONG_STRING_THRESHOLD: usize = 256;

/// Number of characters to retain from the front/back of long JSON strings.
pub const LONG_STRING_EDGE_CHARS: usize = 96;

/// Extra context lines kept around a normal log signal line.
pub const LOG_SIGNAL_CONTEXT_LINES: usize = 1;

/// Extra context lines kept around a stack-trace or panic line.
pub const STACK_TRACE_CONTEXT_LINES: usize = 2;

/// Fallback head lines used when code-like content has no strong boundaries.
pub const CODE_FALLBACK_HEAD_LINES: usize = 4;

/// Fallback tail lines used when code-like content has no strong boundaries.
pub const CODE_FALLBACK_TAIL_LINES: usize = 4;

/// Heuristic score for generic identifier-like JSON keys.
pub const JSON_SCORE_ID_NAME: usize = 1;

/// Heuristic score for endpoint/path-like JSON keys.
pub const JSON_SCORE_ENDPOINT: usize = 8;

/// Heuristic score for status/code-like JSON keys.
pub const JSON_SCORE_STATUS: usize = 6;

/// Heuristic score for error/message-like JSON keys.
pub const JSON_SCORE_ERROR: usize = 10;

/// Heuristic score for warning-like JSON keys.
pub const JSON_SCORE_WARN: usize = 4;

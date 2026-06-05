use crate::defaults::{
    CODE_FALLBACK_HEAD_LINES, CODE_FALLBACK_TAIL_LINES, LOG_SIGNAL_CONTEXT_LINES,
    STACK_TRACE_CONTEXT_LINES,
};

fn is_keep_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let keywords = [
        "error",
        "failed",
        "exception",
        "panic",
        "traceback",
        "backtrace",
        "stack backtrace",
        "caused by",
        "thread '",
        "fatal",
        "warning",
        "warn",
        "info",
        "debug",
        "trace",
        "assert",
        "todo",
        "fixme",
        "status code",
        "response",
        "request",
        "timeout",
        "connection refused",
        "broken pipe",
        "fn ",
        "def ",
        "class ",
        "impl ",
        "struct ",
        "pub ",
        "use ",
        "import ",
    ];
    keywords.iter().any(|marker| lower.contains(marker))
}

fn extra_context_width(line: &str) -> usize {
    let lower = line.to_ascii_lowercase();
    if lower.contains("stack backtrace")
        || lower.contains("backtrace")
        || lower.contains("traceback")
        || lower.contains("caused by")
        || lower.contains("thread '")
        || lower.contains("panicked at")
    {
        STACK_TRACE_CONTEXT_LINES
    } else {
        LOG_SIGNAL_CONTEXT_LINES
    }
}

fn is_code_boundary(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("fn ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub async fn ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("impl ")
        || trimmed.starts_with("pub impl ")
        || trimmed.starts_with("trait ")
        || trimmed.starts_with("pub trait ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("pub struct ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("pub enum ")
        || trimmed.starts_with("use ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("type ")
        || trimmed.starts_with("pub type ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("pub const ")
        || trimmed.starts_with("macro_rules!")
        || trimmed.starts_with("#[")
        || trimmed.starts_with("///")
        || trimmed.starts_with("//")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("loop ")
        || trimmed.starts_with("unsafe ")
}

pub fn compress_text(
    content: &str,
    keep_head: usize,
    keep_tail: usize,
    max_lines: usize,
) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= max_lines {
        return content.to_string();
    }

    let mut kept_idx: Vec<usize> = (0..keep_head.min(lines.len())).collect();
    let body_end = lines.len().saturating_sub(keep_tail.min(lines.len()));
    for (idx, line) in lines
        .iter()
        .enumerate()
        .take(body_end)
        .skip(keep_head.min(lines.len()))
    {
        if is_keep_line(line) {
            kept_idx.push(idx);
            let extra = extra_context_width(line);
            for offset in 1..=extra {
                if idx >= offset {
                    kept_idx.push(idx - offset);
                }
                if idx + offset < body_end {
                    kept_idx.push(idx + offset);
                }
            }
        }
    }

    for idx in lines.len().saturating_sub(keep_tail.min(lines.len()))..lines.len() {
        kept_idx.push(idx);
    }

    kept_idx.sort_unstable();
    kept_idx.dedup();

    if kept_idx.len() > max_lines {
        let mut sampled = Vec::new();
        let step = ((kept_idx.len() as f64) / (max_lines as f64)).ceil() as usize;
        let mut cursor = 0usize;
        while cursor < kept_idx.len() && sampled.len() < max_lines {
            sampled.push(kept_idx[cursor]);
            cursor = cursor.saturating_add(step.max(1));
        }
        kept_idx = sampled;
    }

    let mut kept: Vec<String> = Vec::new();
    let mut last = None;
    for idx in kept_idx {
        if let Some(prev) = last {
            if idx > prev + 1 {
                kept.push(format!("... [{} lines omitted] ...", idx - prev - 1));
            }
        }
        kept.push(lines[idx].to_string());
        last = Some(idx);
    }

    if kept.is_empty() {
        return content.to_string();
    }

    kept.join("\n")
}

pub fn compress_code(content: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= max_lines {
        return content.to_string();
    }

    let mut kept_idx: Vec<usize> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if is_code_boundary(line) {
            kept_idx.push(idx);
            if idx > 0 {
                kept_idx.push(idx - 1);
            }
            if idx + 1 < lines.len() {
                kept_idx.push(idx + 1);
            }
        }
    }
    if kept_idx.is_empty() {
        return compress_text(
            content,
            CODE_FALLBACK_HEAD_LINES,
            CODE_FALLBACK_TAIL_LINES,
            max_lines,
        );
    }

    kept_idx.sort_unstable();
    kept_idx.dedup();
    if kept_idx.len() > max_lines {
        let mut sampled = Vec::new();
        let step = ((kept_idx.len() as f64) / (max_lines as f64)).ceil() as usize;
        let mut cursor = 0usize;
        while cursor < kept_idx.len() && sampled.len() < max_lines {
            sampled.push(kept_idx[cursor]);
            cursor = cursor.saturating_add(step.max(1));
        }
        kept_idx = sampled;
    }

    let mut kept: Vec<String> = Vec::new();
    let mut last = None;
    for idx in kept_idx {
        if let Some(prev) = last {
            if idx > prev + 1 {
                kept.push(format!("... [{} lines omitted] ...", idx - prev - 1));
            }
        }
        kept.push(lines[idx].to_string());
        last = Some(idx);
    }
    kept.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_code_signatures() {
        let input = r#"
use std::fmt;

pub fn outer() {
    let x = 1;
    if x > 0 {
        println!("hello");
    }
}

fn helper() {
    println!("helper");
}
"#;

        let compressed = compress_code(input, 10);
        assert!(compressed.contains("pub fn outer()"));
        assert!(compressed.contains("fn helper()"));
        assert!(compressed.contains("use std::fmt;"));
    }

    #[test]
    fn keeps_trait_and_match_boundaries() {
        let input = r#"
pub trait Formatter {
    fn format(&self, input: &str) -> String;
}

pub async fn render() {
    match true {
        true => println!("hit"),
        false => println!("miss"),
    }
}

fn helper() {
    for idx in 0..10 {
        println!("{idx}");
    }
}
"#;

        let compressed = compress_code(input, 12);
        assert!(compressed.contains("pub trait Formatter"));
        assert!(compressed.contains("pub async fn render()"));
        assert!(compressed.contains("match true"));
    }

    #[test]
    fn keeps_error_context_around_log_signals() {
        let input = r#"
2026-06-05T12:00:00Z INFO service starting
2026-06-05T12:00:00Z INFO loading config
2026-06-05T12:00:00Z INFO connecting to cache
2026-06-05T12:00:00Z INFO connecting to database
2026-06-05T12:00:00Z INFO preparing request
2026-06-05T12:00:00Z INFO dispatching work
2026-06-05T12:00:01Z ERROR request failed
thread 'worker-1' panicked at src/lib.rs:12:8
stack backtrace:
   0: core::panicking::panic_fmt
   1: worker::run
Caused by: timeout while contacting upstream
2026-06-05T12:00:02Z WARN retrying request
2026-06-05T12:00:02Z DEBUG retry attempt 1
2026-06-05T12:00:02Z DEBUG retry attempt 2
2026-06-05T12:00:02Z INFO retry succeeded
2026-06-05T12:00:02Z INFO service stopped
2026-06-05T12:00:03Z INFO cleanup complete
"#;

        let compressed = compress_text(input, 2, 2, 20);
        assert!(compressed.contains("ERROR request failed"));
        assert!(compressed.contains("stack backtrace"));
        assert!(compressed.contains("Caused by: timeout while contacting upstream"));
    }
}

//! Context compression utilities for rUvOS.
//!
//! The public API is intentionally small:
//! - `compress_content`
//! - `detect_content_type`
//! - `store_original`
//! - `retrieve_original`

mod ccr;
mod detect;
mod eval;
mod json;
mod pipeline;
mod text;

pub mod defaults;

pub use ccr::{retrieve_original, store_original};
pub use detect::{detect_content_type, ContentKind};
pub use eval::{
    compare_regression_reports, default_regression_cases, load_regression_report,
    run_compression_regression_suite, CompressionRegressionCase, CompressionRegressionCaseResult,
    CompressionRegressionComparison, CompressionRegressionReport, CompressionRegressionSummary,
    CompressionRegressionSummaryDelta,
};
pub use pipeline::{
    compress_content, compress_content_into_session, CompressionConfig, CompressionResult,
};

#[cfg(test)]
mod tests {
    use super::*;
    use ruvos_session::Session;

    #[test]
    fn detects_json() {
        assert_eq!(detect_content_type(r#"{"a":[1,2,3]}"#), ContentKind::Json);
    }

    #[test]
    fn compresses_text() {
        let result = compress_content(
            "line one\nline two\nline three\nline four\nline five\nline six\nline seven\nline eight\nline nine\nline ten\nline eleven",
            None,
            CompressionConfig::default(),
        );
        assert!(result.compressed.len() <= result.original.len());
    }

    #[tokio::test]
    async fn persists_original_into_session_state() {
        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("test.rvf");
        let mut session = Session::new();
        session.name = "compress-test".into();
        session.rvf_path = session_path.to_string_lossy().into_owned();
        ruvos_session::write_session(&session, session.rvf_path.as_str())
            .await
            .unwrap();

        let sample = (1..=100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let result = compress_content_into_session(
            &sample,
            Some(ContentKind::Text),
            CompressionConfig {
                min_bytes: 1,
                keep_head_lines: 2,
                keep_tail_lines: 2,
                max_array_items: 12,
            },
            Some(session.rvf_path.as_str()),
        )
        .await
        .unwrap();

        assert!(result.changed);
        let loaded = ruvos_session::read_session(session.rvf_path.as_str())
            .await
            .unwrap();
        let reference = result.original_ref.expect("reference present");
        let stored = loaded
            .state
            .get(&format!("compress.original.{reference}"))
            .cloned()
            .expect("stored original");
        assert_eq!(stored, serde_json::to_string(&result.original).unwrap());
    }

    #[tokio::test]
    async fn persists_json_original_into_session_state() {
        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("json.rvf");
        let mut session = Session::new();
        session.name = "compress-json-test".into();
        session.rvf_path = session_path.to_string_lossy().into_owned();
        ruvos_session::write_session(&session, session.rvf_path.as_str())
            .await
            .unwrap();

        let sample = serde_json::json!([
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
        ])
        .to_string();

        let result = compress_content_into_session(
            &sample,
            Some(ContentKind::Json),
            CompressionConfig {
                min_bytes: 1,
                keep_head_lines: 2,
                keep_tail_lines: 2,
                max_array_items: 6,
            },
            Some(session.rvf_path.as_str()),
        )
        .await
        .unwrap();

        assert!(result.changed);
        let loaded = ruvos_session::read_session(session.rvf_path.as_str())
            .await
            .unwrap();
        let reference = result.original_ref.expect("reference present");
        let stored = loaded
            .state
            .get(&format!("compress.original.{reference}"))
            .cloned()
            .expect("stored original");
        assert_eq!(stored, serde_json::to_string(&result.original).unwrap());
    }

    #[tokio::test]
    async fn persists_code_original_into_session_state() {
        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("code.rvf");
        let mut session = Session::new();
        session.name = "compress-code-test".into();
        session.rvf_path = session_path.to_string_lossy().into_owned();
        ruvos_session::write_session(&session, session.rvf_path.as_str())
            .await
            .unwrap();

        let mut sample = String::from("use std::fmt;\n\npub fn outer() {\n");
        for i in 0..40 {
            sample.push_str(&format!("    let filler_{i} = {i};\n"));
        }
        sample.push_str("}\n\nfn helper() {\n");
        for i in 0..40 {
            sample.push_str(&format!("    let helper_{i} = {i};\n"));
        }
        sample.push_str("}\n");

        let result = compress_content_into_session(
            &sample,
            Some(ContentKind::Code),
            CompressionConfig {
                min_bytes: 1,
                keep_head_lines: 2,
                keep_tail_lines: 2,
                max_array_items: 12,
            },
            Some(session.rvf_path.as_str()),
        )
        .await
        .unwrap();

        assert!(result.changed);
        let loaded = ruvos_session::read_session(session.rvf_path.as_str())
            .await
            .unwrap();
        let reference = result.original_ref.expect("reference present");
        let stored = loaded
            .state
            .get(&format!("compress.original.{reference}"))
            .cloned()
            .expect("stored original");
        assert_eq!(stored, serde_json::to_string(&result.original).unwrap());
    }
}

//! Command handlers for rUvOS CLI.

pub mod compress;
pub mod contracts;
pub mod cve;
pub mod doctor;
pub mod eval;
pub mod hook;
pub mod init;
pub mod init_hooks;
pub mod mcp;
pub mod skills;

/// Test-only: serialize tests that touch the process-global `RUVOS_HOME`
/// env var (it is read by `ruvos_mcp::paths::data_root()` at call time, so
/// parallel tests setting it to different tempdirs race).
#[cfg(test)]
pub(crate) async fn ruvos_home_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    LOCK.lock().await
}

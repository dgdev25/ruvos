use anyhow::Result;

pub struct HookQueue;

impl HookQueue {
    pub fn new(_db_path: &str) -> Result<Self> {
        Ok(HookQueue)
    }
}

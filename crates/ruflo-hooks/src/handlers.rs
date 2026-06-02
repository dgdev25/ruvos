pub struct HookDispatcher;

impl HookDispatcher {
    pub fn new() -> Self {
        HookDispatcher
    }
}

impl Default for HookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

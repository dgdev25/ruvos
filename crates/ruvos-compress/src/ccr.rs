use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;

static ORIGINALS: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn short_ref(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    hex::encode(&digest[..12])
}

pub fn store_original(content: &str) -> String {
    let reference = short_ref(content);
    let mut guard = ORIGINALS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.insert(reference.clone(), content.to_string());
    reference
}

pub fn retrieve_original(reference: &str) -> Option<String> {
    let guard = ORIGINALS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get(reference).cloned()
}

pub fn original_reference(content: &str) -> String {
    short_ref(content)
}

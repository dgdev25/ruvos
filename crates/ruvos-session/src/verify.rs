//! `.rvf` signature: real HMAC-SHA256 signing + verification.

use crate::{signing_key, RvfContainer, Session};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Compute the hex HMAC-SHA256 signature over a session's canonical bytes.
pub fn sign_payload(session: &Session) -> String {
    let mut mac =
        HmacSha256::new_from_slice(&signing_key()).expect("HMAC accepts keys of any length");
    mac.update(&session.canonical_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Verify a loaded container: recompute the HMAC over its payload and compare
/// to the stored signature. Returns true only if they match exactly.
pub fn verify_container(container: &RvfContainer) -> bool {
    let mut mac = match HmacSha256::new_from_slice(&signing_key()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(&container.payload.canonical_bytes());
    let expected = match hex::decode(&container.signature) {
        Ok(b) => b,
        Err(_) => return false,
    };
    // `verify_slice` is a constant-time comparison.
    mac.verify_slice(&expected).is_ok()
}

/// Verify the signature chain of an `.rvf` container on disk.
pub async fn verify_signature(rvf_path: &str) -> anyhow::Result<bool> {
    let bytes = tokio::fs::read(rvf_path).await?;
    let container: RvfContainer = serde_json::from_slice(&bytes)?;
    Ok(verify_container(&container))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_roundtrips_and_detects_tampering() {
        let mut s = Session::new();
        s.name = "demo".into();
        s.state.insert("k".into(), "\"v\"".into());

        let sig = sign_payload(&s);
        let container = RvfContainer {
            version: "rvf-1".into(),
            payload: s.clone(),
            signature: sig,
        };
        assert!(verify_container(&container), "valid container must verify");

        // Tamper with the payload — signature must no longer match.
        let mut tampered = container.clone();
        tampered.payload.name = "evil".into();
        assert!(
            !verify_container(&tampered),
            "tampered payload must fail verification"
        );
    }
}

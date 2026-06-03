//! Self-contained FNV-1a feature-hashing embedder.
//!
//! Deliberately NOT imported from ruvos-mcp to avoid a circular crate
//! dependency.  The algorithm is identical — see ruvos-mcp's `embedding.rs`.

/// Embedding dimensionality: matches sentence-embedding convention (384-d).
pub const EMBED_DIM: usize = 384;

/// FNV-1a 64-bit hash — stable across runs, unlike `DefaultHasher`.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Lowercase alphanumeric word tokens.
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Feature-hashing embedding: hash each token into a bucket with a signed
/// contribution, then L2-normalize.  Deterministic and fully offline.
pub fn embed(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; EMBED_DIM];
    for tok in tokenize(text) {
        let h = fnv1a(tok.as_bytes());
        let idx = (h % EMBED_DIM as u64) as usize;
        let sign = if (h >> 7) & 1 == 0 { 1.0 } else { -1.0 };
        v[idx] += sign;
    }
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

/// Cosine similarity for two L2-normalized dense vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_is_normalized_and_deterministic() {
        let a = embed("temporal knowledge graph entity");
        let b = embed("temporal knowledge graph entity");
        assert_eq!(a, b);
        let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
    }

    #[test]
    fn similar_texts_score_higher() {
        let q = embed("database schema migration");
        let near = embed("schema migration database");
        let far = embed("react button css styling");
        assert!(cosine(&q, &near) > cosine(&q, &far));
    }
}

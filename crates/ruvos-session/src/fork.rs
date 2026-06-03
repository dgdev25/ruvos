//! COW-branch session forking — read parent, write a new child container.

use crate::rvf::{read_session, write_session};
use crate::Session;
use uuid::Uuid;

/// Fork a session for parallel exploration using copy-on-write semantics.
///
/// Reads the parent `.rvf` (verifying its signature), creates a new session
/// that inherits the parent's state snapshot, links back to the parent, and
/// writes a fresh signed `.rvf` for the child. `base_dir` is where the new
/// container is written (e.g. the `.rvf` data directory).
pub async fn fork_session(parent_path: &str, base_dir: &str) -> anyhow::Result<Session> {
    let parent = read_session(parent_path).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let child_id = Uuid::new_v4();
    let child_path = format!("{}/{}.rvf", base_dir.trim_end_matches('/'), child_id);

    let child = Session {
        id: child_id,
        rvf_path: child_path.clone(),
        name: format!("{}-fork", parent.name),
        created_at: now.clone(),
        updated_at: now,
        parent: Some(parent.id),
        // COW: inherit the parent's state snapshot; future mutations are isolated.
        state: parent.state.clone(),
    };

    write_session(&child, &child_path).await?;
    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn fork_inherits_state_and_links_parent() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_str().unwrap();

        let mut parent = Session::new();
        parent.name = "parent".into();
        let parent_path = format!("{}/{}.rvf", base, parent.id);
        parent.rvf_path = parent_path.clone();
        let mut seeded = BTreeMap::new();
        seeded.insert("shared".into(), "\"data\"".into());
        parent.state = seeded.clone();
        write_session(&parent, &parent_path).await.unwrap();

        let child = fork_session(&parent_path, base).await.unwrap();

        assert_eq!(child.parent, Some(parent.id), "child must link to parent");
        assert_eq!(child.state, seeded, "child must inherit parent state (COW)");
        assert_ne!(child.id, parent.id, "child must have a distinct id");
        assert!(
            std::path::Path::new(&child.rvf_path).exists(),
            "child .rvf file must be written to disk"
        );
    }
}

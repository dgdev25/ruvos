//! COW-branch session forking — read parent, write a child whose witness chain
//! extends the parent's (real cryptographic lineage).

use crate::rvf::{chain_entries, read_container, write_container};
use crate::Session;
use uuid::Uuid;

/// Fork a session for parallel exploration using copy-on-write semantics.
///
/// Reads and verifies the parent `.rvf`, creates a child that inherits the
/// parent's state snapshot and links back to the parent, then writes the child
/// with a witness chain that **extends the parent's** — so the child's chain
/// cryptographically proves its descent from the parent.
pub async fn fork_session(parent_path: &str, base_dir: &str) -> anyhow::Result<Session> {
    let parent_container = read_container(parent_path).await?;
    let parent = parent_container.payload.clone();
    let parent_entries = chain_entries(&parent_container)?;

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

    // Child chain = parent's entries + a new provenance entry for the child.
    write_container(&child, &parent_entries, &child_path).await?;
    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rvf::write_session;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn fork_inherits_state_and_extends_lineage() {
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

        // Child container verifies and its chain extends the parent's (2 entries).
        let child_container = read_container(&child.rvf_path).await.unwrap();
        let entries = chain_entries(&child_container).unwrap();
        assert_eq!(entries.len(), 2, "child lineage extends the parent chain");
    }
}

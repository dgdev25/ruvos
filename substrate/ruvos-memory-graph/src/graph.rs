//! `MemoryGraph` — the central temporal knowledge graph struct.
//!
//! Wraps a `petgraph::Graph<EntityNode, EntityEdge, Directed>` and exposes the
//! graphiti-inspired API: add_episode, search, get_entity, get_relations,
//! invalidate_fact, active_facts_at.  Persistence is delegated to `persist`.

use crate::edge::EntityEdge;
use crate::extract::{extract_co_occurrences, extract_entities};
use crate::graph_embed::{cosine, embed};
use crate::node::{EntityNode, Episode};
use crate::persist::{self, GraphStore};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Temporal knowledge graph backed by petgraph + JSON persistence.
pub struct MemoryGraph {
    /// Inner directed graph.  Node weights are `EntityNode`; edge weights are
    /// `EntityEdge`.
    inner: DiGraph<EntityNode, EntityEdge>,
    /// All ingested episodes (raw inputs), stored separately from the graph.
    episodes: Vec<Episode>,
    /// Name → NodeIndex lookup for O(1) entity resolution.
    name_index: HashMap<String, NodeIndex>,
    /// Edge UUID → EdgeIndex for O(1) edge lookup.
    edge_index: HashMap<Uuid, petgraph::graph::EdgeIndex>,
    /// Path to the JSON backing file.  If `None`, graph is in-memory only.
    data_path: Option<PathBuf>,
}

impl MemoryGraph {
    // ── Constructors ─────────────────────────────────────────────────────────

    /// Create an in-memory graph (not persisted).
    pub fn in_memory() -> Self {
        Self {
            inner: DiGraph::new(),
            episodes: Vec::new(),
            name_index: HashMap::new(),
            edge_index: HashMap::new(),
            data_path: None,
        }
    }

    /// Open (or create) a persisted graph at `path`.  Replays stored nodes and
    /// edges into the live petgraph structure.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let data_path = path.as_ref().to_path_buf();
        let store = persist::load(&data_path)?;
        let mut g = Self {
            inner: DiGraph::new(),
            episodes: store.episodes,
            name_index: HashMap::new(),
            edge_index: HashMap::new(),
            data_path: Some(data_path),
        };
        // Replay nodes first so we can build the name index.
        for node in store.nodes {
            let idx = g.inner.add_node(node.clone());
            g.name_index.insert(node.name.to_lowercase(), idx);
        }
        // Replay edges, resolving source/target via their stored ids.
        let node_by_id: HashMap<Uuid, NodeIndex> =
            g.inner.node_indices().map(|i| (g.inner[i].id, i)).collect();
        for edge in store.edges {
            let src = node_by_id
                .get(&edge.source_id)
                .copied()
                .ok_or_else(|| anyhow!("missing source node {}", edge.source_id))?;
            let tgt = node_by_id
                .get(&edge.target_id)
                .copied()
                .ok_or_else(|| anyhow!("missing target node {}", edge.target_id))?;
            let eid = edge.id;
            let ei = g.inner.add_edge(src, tgt, edge);
            g.edge_index.insert(eid, ei);
        }
        Ok(g)
    }

    // ── Core API ─────────────────────────────────────────────────────────────

    /// Ingest a raw episode: extract entities and co-occurrence relations, then
    /// upsert them into the graph.  Returns the new `Episode`.
    pub fn add_episode(
        &mut self,
        content: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Episode> {
        let content = content.into();
        let source = source.into();
        let episode = Episode::new(content.clone(), source);
        self.episodes.push(episode.clone());

        // Extract entities — upsert by lower-cased name.
        let entity_names = extract_entities(&content);
        for name in &entity_names {
            self.upsert_entity(name, &content);
        }

        // Extract co-occurring pairs → directed edges.
        let pairs = extract_co_occurrences(&content);
        for (a, b) in pairs {
            let fact = format!("{} co-occurs with {} in episode {}", a, b, episode.id);
            self.add_relation(&a, &b, "co-occurs", &fact)?;
        }

        self.flush()?;
        Ok(episode)
    }

    /// Hybrid search: vector similarity across all entity names+summaries, then
    /// BFS expansion from the top match.  Returns up to `k` `EntityNode`s.
    pub fn search(&self, query: &str, k: usize) -> Vec<&EntityNode> {
        if k == 0 {
            return Vec::new();
        }
        let qvec = embed(query);

        // Score every node by cosine similarity of its searchable text.
        let mut scored: Vec<(NodeIndex, f32)> = self
            .inner
            .node_indices()
            .map(|i| {
                let n = &self.inner[i];
                let text = format!("{} {}", n.name, n.summary);
                let sim = cosine(&qvec, &embed(&text));
                (i, sim)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if scored.is_empty() {
            return Vec::new();
        }

        // BFS from the highest-similarity seed, collecting up to k nodes.
        let seed = scored[0].0;
        let mut visited: std::collections::HashSet<NodeIndex> = std::collections::HashSet::new();
        let mut queue: VecDeque<NodeIndex> = VecDeque::new();
        queue.push_back(seed);

        let mut results: Vec<&EntityNode> = Vec::new();
        while let Some(idx) = queue.pop_front() {
            if visited.contains(&idx) || results.len() >= k {
                continue;
            }
            visited.insert(idx);
            results.push(&self.inner[idx]);

            // Enqueue neighbours (both outgoing and incoming edges).
            for edge in self.inner.edges(idx) {
                queue.push_back(edge.target());
            }
            for edge in self
                .inner
                .edges_directed(idx, petgraph::Direction::Incoming)
            {
                queue.push_back(edge.source());
            }
        }

        // If BFS didn't fill k slots, pad with next-best scored nodes.
        for (ni, _) in &scored {
            if results.len() >= k {
                break;
            }
            if !visited.contains(ni) {
                results.push(&self.inner[*ni]);
                visited.insert(*ni);
            }
        }

        results
    }

    /// Look up an entity by exact name (case-insensitive).
    pub fn get_entity(&self, name: &str) -> Option<&EntityNode> {
        self.name_index
            .get(&name.to_lowercase())
            .map(|&i| &self.inner[i])
    }

    /// Return all edges (in either direction) incident to the named entity.
    pub fn get_relations(&self, entity_name: &str) -> Vec<&EntityEdge> {
        let idx = match self.name_index.get(&entity_name.to_lowercase()) {
            Some(&i) => i,
            None => return Vec::new(),
        };

        let mut edges: Vec<&EntityEdge> = Vec::new();

        for er in self.inner.edges(idx) {
            edges.push(er.weight());
        }
        for er in self
            .inner
            .edges_directed(idx, petgraph::Direction::Incoming)
        {
            edges.push(er.weight());
        }

        edges
    }

    /// Mark the edge with `edge_id` as invalid from `at` onward.
    pub fn invalidate_fact(&mut self, edge_id: Uuid, at: DateTime<Utc>) -> Result<()> {
        let ei = self
            .edge_index
            .get(&edge_id)
            .copied()
            .ok_or_else(|| anyhow!("edge {} not found", edge_id))?;
        self.inner[ei].invalid_at = Some(at);
        self.flush()
    }

    /// Return all edges that are active (valid) at `time`.
    pub fn active_facts_at(&self, time: DateTime<Utc>) -> Vec<&EntityEdge> {
        self.inner
            .edge_indices()
            .map(|i| &self.inner[i])
            .filter(|e| e.is_active_at(time))
            .collect()
    }

    /// Total number of entity nodes.
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Total number of edges.
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    /// Upsert an entity by (lower-cased) name.  If the entity already exists,
    /// updates `updated_at`.  Returns the `NodeIndex`.
    fn upsert_entity(&mut self, name: &str, _context: &str) -> NodeIndex {
        let key = name.to_lowercase();
        if let Some(&idx) = self.name_index.get(&key) {
            self.inner[idx].updated_at = Utc::now();
            return idx;
        }
        let node = EntityNode::new(name);
        let idx = self.inner.add_node(node);
        self.name_index.insert(key, idx);
        idx
    }

    /// Add a directed edge between named entities (creating them if absent).
    /// Silently skips self-loops.
    fn add_relation(
        &mut self,
        src_name: &str,
        tgt_name: &str,
        rel: &str,
        fact: &str,
    ) -> Result<()> {
        let src_key = src_name.to_lowercase();
        let tgt_key = tgt_name.to_lowercase();
        if src_key == tgt_key {
            return Ok(());
        }
        // Upsert both endpoints.
        let src_idx = self.upsert_entity(src_name, "");
        let tgt_idx = self.upsert_entity(tgt_name, "");

        let src_id = self.inner[src_idx].id;
        let tgt_id = self.inner[tgt_idx].id;
        let edge = EntityEdge::new(src_id, tgt_id, rel, fact);
        let eid = edge.id;
        let ei = self.inner.add_edge(src_idx, tgt_idx, edge);
        self.edge_index.insert(eid, ei);
        Ok(())
    }

    /// Persist current state to disk (no-op for in-memory graphs).
    fn flush(&self) -> Result<()> {
        let path = match &self.data_path {
            Some(p) => p,
            None => return Ok(()),
        };

        let nodes: Vec<crate::node::EntityNode> = self
            .inner
            .node_indices()
            .map(|i| self.inner[i].clone())
            .collect();

        let edges: Vec<EntityEdge> = self
            .inner
            .edge_indices()
            .map(|i| self.inner[i].clone())
            .collect();

        persist::save(
            path,
            &GraphStore {
                nodes,
                edges,
                episodes: self.episodes.clone(),
            },
        )
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // 1. add_episode creates nodes and edges
    #[test]
    fn add_episode_creates_nodes_and_edges() {
        let mut g = MemoryGraph::in_memory();
        g.add_episode("Alice met Bob at the conference in London.", "test")
            .unwrap();

        assert!(g.node_count() > 0, "episode must create at least one node");
        // At least Alice and Bob should be extracted as named entities.
        assert!(
            g.get_entity("Alice").is_some(),
            "Alice must be extracted as an entity"
        );
        assert!(
            g.get_entity("Bob").is_some(),
            "Bob must be extracted as an entity"
        );
        // Alice-Bob co-occurrence → at least one edge.
        assert!(
            g.edge_count() > 0,
            "co-occurrence must create at least one edge"
        );
    }

    // 2. search finds the relevant entity
    #[test]
    fn search_finds_relevant_entity() {
        let mut g = MemoryGraph::in_memory();
        g.add_episode(
            "PostgreSQL is a relational database used by engineers.",
            "tech-docs",
        )
        .unwrap();
        g.add_episode(
            "Monet painted water lilies in his garden at Giverny.",
            "art-docs",
        )
        .unwrap();

        let results = g.search("database relational storage", 3);
        assert!(!results.is_empty(), "search must return results");
        // The top hit should be database-related, not art-related.
        let top_name = results[0].name.to_lowercase();
        let db_terms = ["postgresql", "database", "engineers", "relational"];
        assert!(
            db_terms.iter().any(|t| top_name.contains(t)),
            "top result '{}' should be database-related",
            top_name
        );
    }

    // 3. invalidate_fact + active_facts_at respects timestamp
    #[test]
    fn invalidate_and_query_at_time() {
        let mut g = MemoryGraph::in_memory();
        g.add_episode("Alice works at Acme.", "hr").unwrap();

        // Grab the first edge's id.
        let edge_ids: Vec<Uuid> = g.inner.edge_indices().map(|i| g.inner[i].id).collect();
        assert!(!edge_ids.is_empty(), "must have at least one edge");

        let target_id = edge_ids[0];

        // t_current is "now" — all edges were just created so they're active.
        let t_current = Utc::now() + Duration::milliseconds(1);
        // Invalidate slightly in the future so the edge was active up to that
        // point and inactive strictly after it.
        let t_invalidation = t_current + Duration::seconds(1);
        let t_after = t_invalidation + Duration::seconds(1);

        // All edges should be active at t_current (just after creation).
        let active_now = g.active_facts_at(t_current);
        assert!(
            active_now.iter().any(|e| e.id == target_id),
            "edge should be active before invalidation"
        );

        g.invalidate_fact(target_id, t_invalidation).unwrap();

        // After the invalidation timestamp, the fact should no longer appear.
        let active_after = g.active_facts_at(t_after);
        assert!(
            !active_after.iter().any(|e| e.id == target_id),
            "edge should not be active after invalidation timestamp"
        );
    }

    // 4. persist roundtrip: save and reload preserves node/edge counts
    #[test]
    fn persist_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.json");

        let (node_count, edge_count) = {
            let mut g = MemoryGraph::open(&path).unwrap();
            g.add_episode("Alice met Bob and Carol in Paris.", "test")
                .unwrap();
            g.add_episode("Bob joined Acme Corp recently.", "test")
                .unwrap();
            (g.node_count(), g.edge_count())
        };

        // Reload from disk in a fresh graph.
        let g2 = MemoryGraph::open(&path).unwrap();
        assert_eq!(
            g2.node_count(),
            node_count,
            "node count must survive roundtrip"
        );
        assert_eq!(
            g2.edge_count(),
            edge_count,
            "edge count must survive roundtrip"
        );
    }

    // 5. get_relations returns incident edges for known entity
    #[test]
    fn get_relations_returns_incident_edges() {
        let mut g = MemoryGraph::in_memory();
        g.add_episode(
            "Alice and Bob collaborate. Alice and Carol work together.",
            "test",
        )
        .unwrap();

        let rels = g.get_relations("Alice");
        assert!(!rels.is_empty(), "Alice must have at least one relation");
    }

    // 6. get_entity is case-insensitive
    #[test]
    fn get_entity_case_insensitive() {
        let mut g = MemoryGraph::in_memory();
        g.add_episode("PostgreSQL powers the backend.", "docs")
            .unwrap();

        assert!(g.get_entity("postgresql").is_some());
        assert!(g.get_entity("POSTGRESQL").is_some());
        assert!(g.get_entity("PostgreSQL").is_some());
    }
}

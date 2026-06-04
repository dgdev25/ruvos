//! redb-backed skills pack.

use crate::records::{
    hash_bytes, tokenize, CompressionCodec, SkillChunkLink, SkillChunkRecord, SkillFeedbackRecord,
    SkillPackMeta, SkillRecord, SkillSearchHit,
};
use anyhow::Context;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use redb::{Database, ReadableTable, TableDefinition};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

type Result<T> = anyhow::Result<T>;

const PACK_META: TableDefinition<&str, &[u8]> = TableDefinition::new("pack_meta");
const SKILLS: TableDefinition<&str, &[u8]> = TableDefinition::new("skills");
const CHUNKS: TableDefinition<&str, &[u8]> = TableDefinition::new("chunks");
const SKILL_CHUNKS: TableDefinition<&str, &[u8]> = TableDefinition::new("skill_chunks");
const TAG_INDEX: TableDefinition<&str, &[u8]> = TableDefinition::new("tag_index");
const ALIAS_INDEX: TableDefinition<&str, &[u8]> = TableDefinition::new("alias_index");
const TERM_INDEX: TableDefinition<&str, &[u8]> = TableDefinition::new("term_index");
const SOURCE_INDEX: TableDefinition<&str, &[u8]> = TableDefinition::new("source_index");
const FEEDBACK: TableDefinition<&str, &[u8]> = TableDefinition::new("feedback");

/// Portable, transient skills pack handle.
pub struct SkillStore {
    db: Arc<Database>,
}

impl SkillStore {
    /// Open or create a skills pack at `path`, ensuring all tables exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("creating skills pack directory {}", parent.display())
                })?;
            }
        }

        let db = Database::create(path)?;
        let txn = db.begin_write()?;
        {
            let _ = txn.open_table(PACK_META)?;
            let _ = txn.open_table(SKILLS)?;
            let _ = txn.open_table(CHUNKS)?;
            let _ = txn.open_table(SKILL_CHUNKS)?;
            let _ = txn.open_table(TAG_INDEX)?;
            let _ = txn.open_table(ALIAS_INDEX)?;
            let _ = txn.open_table(TERM_INDEX)?;
            let _ = txn.open_table(SOURCE_INDEX)?;
            let _ = txn.open_table(FEEDBACK)?;
        }
        txn.commit()?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Store or update pack metadata.
    pub fn put_pack_meta(&self, meta: &SkillPackMeta) -> Result<()> {
        self.put_json(PACK_META, "__meta__", meta)
    }

    /// Fetch the pack metadata, if present.
    pub fn get_pack_meta(&self) -> Result<Option<SkillPackMeta>> {
        self.get_json(PACK_META, "__meta__")
    }

    /// Insert or update a normalized skill and all of its indexes.
    pub fn put_skill(&self, skill: &SkillRecord) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let old: Option<SkillRecord> = get_json_in_write_txn(&txn, SKILLS, &skill.id)?;
            put_json_in_txn(&txn, SKILLS, &skill.id, skill)?;

            let new_tags = normalize_strings(&skill.tags);
            let new_aliases = normalize_strings(&skill.aliases);
            let new_terms = skill.index_terms();
            let new_sources = normalize_strings(&[
                skill.source.source_root.clone(),
                skill.source.source_path.clone(),
                skill.source.corpus_hash.clone(),
            ]);

            let old_tags: BTreeSet<String> = old
                .as_ref()
                .map_or_else(BTreeSet::new, |s| normalize_strings(&s.tags));
            let old_aliases: BTreeSet<String> = old
                .as_ref()
                .map_or_else(BTreeSet::new, |s| normalize_strings(&s.aliases));
            let old_terms: BTreeSet<String> = old
                .as_ref()
                .map_or_else(BTreeSet::new, SkillRecord::index_terms);
            let old_sources: BTreeSet<String> = old.as_ref().map_or_else(BTreeSet::new, |s| {
                normalize_strings(&[
                    s.source.source_root.clone(),
                    s.source.source_path.clone(),
                    s.source.corpus_hash.clone(),
                ])
            });

            sync_many_to_many_index(&txn, TAG_INDEX, &skill.id, &old_tags, &new_tags)?;
            sync_many_to_many_index(&txn, TERM_INDEX, &skill.id, &old_terms, &new_terms)?;
            sync_many_to_many_index(&txn, SOURCE_INDEX, &skill.id, &old_sources, &new_sources)?;
            sync_alias_index(&txn, &skill.id, &old_aliases, &new_aliases)?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Fetch a skill by id.
    pub fn get_skill(&self, id: &str) -> Result<Option<SkillRecord>> {
        self.get_json(SKILLS, id)
    }

    /// List all skills in deterministic key order.
    pub fn list_skills(&self) -> Result<Vec<SkillRecord>> {
        self.scan_all(SKILLS)
    }

    /// Store a deduplicated chunk payload.
    pub fn put_chunk(&self, chunk: &SkillChunkRecord) -> Result<()> {
        self.put_json(CHUNKS, &chunk.hash, chunk)
    }

    /// Fetch a chunk by content hash.
    pub fn get_chunk(&self, hash: &str) -> Result<Option<SkillChunkRecord>> {
        self.get_json(CHUNKS, hash)
    }

    /// Link a skill to an ordered chunk reference list.
    pub fn put_skill_chunks(&self, skill_id: &str, chunks: &[SkillChunkLink]) -> Result<()> {
        let chunk_vec = chunks.to_vec();
        self.put_json(SKILL_CHUNKS, skill_id, &chunk_vec)
    }

    /// Read the ordered chunk references for a skill.
    pub fn skill_chunks(&self, skill_id: &str) -> Result<Vec<SkillChunkLink>> {
        self.get_json(SKILL_CHUNKS, skill_id)
            .map(|opt| opt.unwrap_or_default())
    }

    /// Add a skill to a tag index.
    pub fn add_tag(&self, tag: &str, skill_id: &str) -> Result<()> {
        self.add_to_set_index(TAG_INDEX, tag, skill_id)
    }

    /// Resolve a tag to the matching skill ids.
    pub fn skills_for_tag(&self, tag: &str) -> Result<Vec<String>> {
        self.get_string_set(TAG_INDEX, tag)
    }

    /// Set an alias to a skill id.
    pub fn set_alias(&self, alias: &str, skill_id: &str) -> Result<()> {
        self.put_string(ALIAS_INDEX, alias, skill_id)
    }

    /// Resolve an alias to a skill id.
    pub fn resolve_alias(&self, alias: &str) -> Result<Option<String>> {
        self.get_string(ALIAS_INDEX, alias)
    }

    /// Add a skill to a term posting list.
    pub fn add_term(&self, term: &str, skill_id: &str) -> Result<()> {
        self.add_to_set_index(TERM_INDEX, term, skill_id)
    }

    /// Search the skills pack using a tokenized lexical query.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SkillSearchHit>> {
        let mut hits: HashMap<String, SkillSearchHit> = HashMap::new();
        for token in tokenize(query) {
            for skill_id in self.get_string_set(TERM_INDEX, &token)? {
                let entry = hits.entry(skill_id.clone()).or_insert_with(|| {
                    SkillSearchHit::new(skill_id.clone(), 0, Vec::new(), "term match")
                });
                entry.score += 1;
                entry.matched_terms.push(token.clone());
            }

            for skill_id in self.get_string_set(TAG_INDEX, &token)? {
                let entry = hits.entry(skill_id.clone()).or_insert_with(|| {
                    SkillSearchHit::new(skill_id.clone(), 0, Vec::new(), "tag match")
                });
                entry.score += 2;
                entry.matched_terms.push(token.clone());
                entry.reason = "tag match".to_string();
            }

            if let Some(skill_id) = self.resolve_alias(&token)? {
                let entry = hits.entry(skill_id.clone()).or_insert_with(|| {
                    SkillSearchHit::new(skill_id.clone(), 0, Vec::new(), "alias match")
                });
                entry.score += 3;
                entry.matched_terms.push(token.clone());
                entry.reason = "alias match".to_string();
            }
        }

        let mut hits: Vec<_> = hits.into_values().collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.skill_id.cmp(&b.skill_id)));
        hits.truncate(limit);
        Ok(hits)
    }

    /// Fetch or initialize feedback for a skill.
    pub fn get_feedback(&self, skill_id: &str) -> Result<Option<SkillFeedbackRecord>> {
        self.get_json(FEEDBACK, skill_id)
    }

    /// Record one usage outcome for a skill.
    pub fn record_feedback(
        &self,
        skill_id: &str,
        success: bool,
        outcome: impl Into<String>,
        note: Option<String>,
    ) -> Result<()> {
        let mut feedback = self
            .get_feedback(skill_id)?
            .unwrap_or_else(|| SkillFeedbackRecord::new(skill_id));
        feedback.usage_count += 1;
        feedback.last_used_at = crate::records::now_secs();
        feedback.last_outcome = Some(outcome.into());
        if success {
            feedback.success_count += 1;
        } else {
            feedback.failure_count += 1;
        }
        if let Some(note) = note {
            feedback.notes.push(note);
        }
        self.put_json(FEEDBACK, skill_id, &feedback)
    }

    /// Build a pack metadata record from the current store contents.
    pub fn build_pack_meta(
        &self,
        corpus_hash: impl Into<String>,
        source_root: impl Into<String>,
        codec: CompressionCodec,
    ) -> Result<SkillPackMeta> {
        let skill_count = self.list_skills()?.len() as u64;
        let chunk_count = self.count_rows(CHUNKS)?;
        Ok(SkillPackMeta::new(
            corpus_hash,
            source_root,
            codec,
            skill_count,
            chunk_count,
        ))
    }

    /// Store a chunk after compressing it with the requested codec.
    pub fn encode_and_put_chunk(
        &self,
        payload: &[u8],
        codec: CompressionCodec,
    ) -> Result<SkillChunkRecord> {
        let (stored_codec, data) = encode_payload(payload, codec)?;
        let hash = hash_bytes(payload);
        let chunk =
            SkillChunkRecord::new(hash.clone(), stored_codec, payload.len(), data.len(), data);
        self.put_chunk(&chunk)?;
        Ok(chunk)
    }

    /// Decode a stored chunk back to its original bytes.
    pub fn decode_chunk(&self, chunk: &SkillChunkRecord) -> Result<Vec<u8>> {
        decode_payload(chunk.codec, &chunk.data)
    }

    fn put_json<T: Serialize>(
        &self,
        table: TableDefinition<&str, &[u8]>,
        key: &str,
        value: &T,
    ) -> Result<()> {
        let txn = self.db.begin_write()?;
        put_json_in_txn(&txn, table, key, value)?;
        txn.commit()?;
        Ok(())
    }

    fn get_json<T: DeserializeOwned>(
        &self,
        table: TableDefinition<&str, &[u8]>,
        key: &str,
    ) -> Result<Option<T>> {
        let txn = self.db.begin_read()?;
        get_json_in_read_txn(&txn, table, key)
    }

    fn put_string(
        &self,
        table: TableDefinition<&str, &[u8]>,
        key: &str,
        value: &str,
    ) -> Result<()> {
        let txn = self.db.begin_write()?;
        put_string_in_txn(&txn, table, key, value)?;
        txn.commit()?;
        Ok(())
    }

    fn get_string(&self, table: TableDefinition<&str, &[u8]>, key: &str) -> Result<Option<String>> {
        let txn = self.db.begin_read()?;
        get_string_in_read_txn(&txn, table, key)
    }

    fn get_string_set(
        &self,
        table: TableDefinition<&str, &[u8]>,
        key: &str,
    ) -> Result<Vec<String>> {
        Ok(self
            .get_json::<Vec<String>>(table, key)?
            .unwrap_or_default())
    }

    fn scan_all<T: DeserializeOwned>(&self, table: TableDefinition<&str, &[u8]>) -> Result<Vec<T>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(table)?;
        let mut out = Vec::new();
        for row in t.iter()? {
            let (_, value) = row?;
            out.push(serde_json::from_slice(value.value())?);
        }
        Ok(out)
    }

    fn count_rows(&self, table: TableDefinition<&str, &[u8]>) -> Result<u64> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(table)?;
        let mut count = 0u64;
        for row in t.iter()? {
            let _ = row?;
            count += 1;
        }
        Ok(count)
    }

    fn add_to_set_index(
        &self,
        table: TableDefinition<&str, &[u8]>,
        key: &str,
        value: &str,
    ) -> Result<()> {
        let txn = self.db.begin_write()?;
        add_to_set_index_in_txn(&txn, table, key, value)?;
        txn.commit()?;
        Ok(())
    }
}

fn normalize_strings(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_ascii_lowercase())
            }
        })
        .collect()
}

fn sync_many_to_many_index(
    txn: &redb::WriteTransaction,
    table: TableDefinition<&str, &[u8]>,
    skill_id: &str,
    old_values: &BTreeSet<String>,
    new_values: &BTreeSet<String>,
) -> Result<()> {
    let to_remove: Vec<_> = old_values.difference(new_values).cloned().collect();
    let to_add: Vec<_> = new_values.difference(old_values).cloned().collect();
    for value in to_remove {
        remove_from_set_index_in_txn(txn, table, &value, skill_id)?;
    }
    for value in to_add {
        add_to_set_index_in_txn(txn, table, &value, skill_id)?;
    }
    Ok(())
}

fn sync_alias_index(
    txn: &redb::WriteTransaction,
    skill_id: &str,
    old_values: &BTreeSet<String>,
    new_values: &BTreeSet<String>,
) -> Result<()> {
    let to_remove: Vec<_> = old_values.difference(new_values).cloned().collect();
    for alias in to_remove {
        remove_alias_if_owned(txn, &alias, skill_id)?;
    }
    for alias in new_values.difference(old_values) {
        put_string_in_txn(txn, ALIAS_INDEX, alias, skill_id)?;
    }
    Ok(())
}

fn encode_payload(payload: &[u8], codec: CompressionCodec) -> Result<(CompressionCodec, Vec<u8>)> {
    match codec {
        CompressionCodec::None => Ok((CompressionCodec::None, payload.to_vec())),
        CompressionCodec::Gzip => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(payload)?;
            let compressed = encoder.finish()?;
            Ok((CompressionCodec::Gzip, compressed))
        }
    }
}

fn decode_payload(codec: CompressionCodec, payload: &[u8]) -> Result<Vec<u8>> {
    match codec {
        CompressionCodec::None => Ok(payload.to_vec()),
        CompressionCodec::Gzip => {
            let mut decoder = GzDecoder::new(payload);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            Ok(out)
        }
    }
}

fn put_json_in_txn<T: Serialize>(
    txn: &redb::WriteTransaction,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let mut t = txn.open_table(table)?;
    t.insert(key, bytes.as_slice())?;
    Ok(())
}

fn get_json_in_read_txn<T: DeserializeOwned>(
    txn: &redb::ReadTransaction,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
) -> Result<Option<T>> {
    let t = txn.open_table(table)?;
    match t.get(key)? {
        Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
        None => Ok(None),
    }
}

fn get_json_in_write_txn<T: DeserializeOwned>(
    txn: &redb::WriteTransaction,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
) -> Result<Option<T>> {
    let t = txn.open_table(table)?;
    let value = match t.get(key)? {
        Some(v) => Some(serde_json::from_slice(v.value())?),
        None => None,
    };
    Ok(value)
}

fn put_string_in_txn(
    txn: &redb::WriteTransaction,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
    value: &str,
) -> Result<()> {
    let mut t = txn.open_table(table)?;
    t.insert(key, value.as_bytes())?;
    Ok(())
}

fn get_string_in_read_txn(
    txn: &redb::ReadTransaction,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
) -> Result<Option<String>> {
    let t = txn.open_table(table)?;
    match t.get(key)? {
        Some(v) => Ok(Some(String::from_utf8(v.value().to_vec())?)),
        None => Ok(None),
    }
}

fn get_string_set_in_txn(
    txn: &redb::WriteTransaction,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
) -> Result<Vec<String>> {
    let t = txn.open_table(table)?;
    let values = match t.get(key)? {
        Some(v) => serde_json::from_slice(v.value())?,
        None => Vec::new(),
    };
    Ok(values)
}

fn add_to_set_index_in_txn(
    txn: &redb::WriteTransaction,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
    value: &str,
) -> Result<()> {
    let mut set = get_string_set_in_txn(txn, table, key)?;
    if !set.iter().any(|item| item == value) {
        set.push(value.to_string());
        set.sort();
        set.dedup();
        put_json_in_txn(txn, table, key, &set)?;
    }
    Ok(())
}

fn remove_from_set_index_in_txn(
    txn: &redb::WriteTransaction,
    table: TableDefinition<&str, &[u8]>,
    key: &str,
    value: &str,
) -> Result<()> {
    let mut set = get_string_set_in_txn(txn, table, key)?;
    let before = set.len();
    set.retain(|item| item != value);
    if set.is_empty() {
        let mut t = txn.open_table(table)?;
        let _ = t.remove(key)?;
    } else if set.len() != before {
        put_json_in_txn(txn, table, key, &set)?;
    }
    Ok(())
}

fn remove_alias_if_owned(txn: &redb::WriteTransaction, alias: &str, skill_id: &str) -> Result<()> {
    let mut t = txn.open_table(ALIAS_INDEX)?;
    let should_remove = match t.get(alias)? {
        Some(existing) => existing.value() == skill_id.as_bytes(),
        None => false,
    };
    if should_remove {
        let _ = t.remove(alias)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::{CompressionCodec, SkillSource};

    fn sample_skill(id: &str) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: "Rust API Review".to_string(),
            version: "1.0.0".to_string(),
            purpose: "Review Rust APIs for correctness".to_string(),
            tags: vec!["rust".to_string(), "review".to_string()],
            aliases: vec!["api-review".to_string()],
            prerequisites: vec!["rust basics".to_string()],
            safety_level: "advisory".to_string(),
            validation: vec!["compile the crate".to_string()],
            summary: Some("A focused review skill.".to_string()),
            source: SkillSource {
                source_root: "/tmp/skillbase".to_string(),
                source_path: "rust/review.md".to_string(),
                corpus_hash: "abc123".to_string(),
            },
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn store_round_trips_skill_metadata_and_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillStore::open(dir.path().join("skills.redb")).unwrap();

        let skill = sample_skill("skill-1");
        store.put_skill(&skill).unwrap();

        assert_eq!(store.get_skill("skill-1").unwrap().unwrap(), skill);
        assert_eq!(
            store.skills_for_tag("rust").unwrap(),
            vec!["skill-1".to_string()]
        );
        assert_eq!(
            store.resolve_alias("api-review").unwrap(),
            Some("skill-1".to_string())
        );

        let hits = store.search("rust api review", 10).unwrap();
        assert_eq!(hits[0].skill_id, "skill-1");
    }

    #[test]
    fn chunk_storage_dedupes_by_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillStore::open(dir.path().join("skills.redb")).unwrap();

        let payload = b"hello skill chunk";
        let chunk = store
            .encode_and_put_chunk(payload, CompressionCodec::None)
            .unwrap();
        store.put_chunk(&chunk).unwrap();
        store.put_chunk(&chunk).unwrap();

        assert_eq!(store.get_chunk(&chunk.hash).unwrap().unwrap(), chunk);
        assert_eq!(store.count_rows(CHUNKS).unwrap(), 1);
        assert_eq!(store.decode_chunk(&chunk).unwrap(), payload);
    }

    #[test]
    fn pack_meta_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillStore::open(dir.path().join("skills.redb")).unwrap();

        let meta = SkillPackMeta::new("corpushash", "/src", CompressionCodec::Gzip, 4, 9);
        store.put_pack_meta(&meta).unwrap();

        assert_eq!(store.get_pack_meta().unwrap().unwrap(), meta);
    }
}

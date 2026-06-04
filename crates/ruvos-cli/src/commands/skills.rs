//! Skill corpus auditing and manifest generation.
//!
//! This command scans the source corpus SQLite database, computes deterministic
//! fingerprints and summary stats, and writes a manifest that can later drive
//! the `skills.redb` pack builder.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::ValueEnum;
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use ruvos_skills::{
    hash_bytes, CompressionCodec, SkillChunkLink, SkillChunkRecord, SkillPackMeta, SkillRecord,
    SkillSource, SkillStore,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum SkillsAuditFormat {
    Json,
    Markdown,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, ValueEnum, Serialize, Deserialize)]
pub enum SkillsPackTier {
    Core,
    Domain,
    Archive,
}

impl std::fmt::Display for SkillsPackTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => f.write_str("core"),
            Self::Domain => f.write_str("domain"),
            Self::Archive => f.write_str("archive"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillsAuditManifest {
    pub version: String,
    pub source_db: String,
    pub corpus_root: String,
    pub generated_at: String,
    pub summary: SkillsAuditSummary,
    pub skills: Vec<SkillAuditRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillsAuditSummary {
    pub total_skills: usize,
    pub total_files: usize,
    pub total_bytes: u64,
    pub duplicate_clusters: usize,
    pub core_candidates: usize,
    pub domain_candidates: usize,
    pub archive_candidates: usize,
    pub excluded_candidates: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillAuditRecord {
    pub db_id: i64,
    pub source: String,
    pub skill_id: String,
    pub name: String,
    pub installs: i64,
    pub page_url: Option<String>,
    pub repo_url: Option<String>,
    pub status: String,
    pub file_count: usize,
    pub markdown_files: usize,
    pub script_files: usize,
    pub total_bytes: u64,
    pub has_skill_md: bool,
    pub canonical_file: Option<String>,
    pub fingerprint: String,
    pub duplicate_cluster_size: usize,
    pub estimated_score: i64,
    pub suggested_tier: SkillTier,
    pub suggested_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillTier {
    Core,
    Domain,
    Archive,
    Exclude,
}

impl std::fmt::Display for SkillTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => f.write_str("core"),
            Self::Domain => f.write_str("domain"),
            Self::Archive => f.write_str("archive"),
            Self::Exclude => f.write_str("exclude"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillsAuditReport {
    pub manifest: SkillsAuditManifest,
    pub top_skills: Vec<SkillAuditRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillsPackReport {
    pub source_manifest: PathBuf,
    pub source_db: PathBuf,
    pub output: PathBuf,
    pub selection_manifest: Option<PathBuf>,
    pub selected_tiers: Vec<SkillsPackTier>,
    pub selected_skills: usize,
    pub selected_chunks: usize,
    pub stored_bytes: u64,
    pub corpus_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillsInstallReport {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub bytes_copied: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackBuildConfig {
    pub manifest_path: PathBuf,
    pub source_db: PathBuf,
    pub output: PathBuf,
    pub selection_manifest: Option<PathBuf>,
    pub selected_tiers: Vec<SkillsPackTier>,
}

pub fn audit(
    corpus_root: impl AsRef<Path>,
    source_db: impl AsRef<Path>,
    write: Option<PathBuf>,
    format: SkillsAuditFormat,
) -> anyhow::Result<SkillsAuditReport> {
    let corpus_root = corpus_root.as_ref().to_path_buf();
    let source_db = source_db.as_ref().to_path_buf();
    let manifest = build_manifest(&corpus_root, &source_db)?;
    let report = build_report(manifest);
    let rendered = match format {
        SkillsAuditFormat::Json => serde_json::to_string_pretty(&report.manifest)?,
        SkillsAuditFormat::Markdown => render_markdown(&report),
    };

    if let Some(path) = write {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&path, rendered).with_context(|| format!("writing {}", path.display()))?;
    } else {
        println!("{rendered}");
    }

    Ok(report)
}

pub fn build_pack(config: PackBuildConfig) -> anyhow::Result<SkillsPackReport> {
    let manifest_text = std::fs::read_to_string(&config.manifest_path)
        .with_context(|| format!("reading {}", config.manifest_path.display()))?;
    let manifest: SkillsAuditManifest = serde_json::from_str(&manifest_text)
        .with_context(|| format!("parsing {}", config.manifest_path.display()))?;
    let source_db =
        Connection::open_with_flags(&config.source_db, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("opening {}", config.source_db.display()))?;
    let store = SkillStore::open(&config.output)?;
    let selection_manifest = config.selection_manifest.clone();
    let uses_curated_manifest = selection_manifest.is_some();

    let selected_tiers = if config.selected_tiers.is_empty() {
        vec![SkillsPackTier::Core]
    } else {
        config.selected_tiers.clone()
    };

    let selected = if let Some(selection_manifest) = &selection_manifest {
        let curated = load_selection_manifest(selection_manifest)?;
        select_manifest_skills_by_ids(&manifest, &curated.selected_skill_ids)?
    } else {
        select_manifest_skills(&manifest, &selected_tiers)
    };

    let mut unique_chunk_hashes = BTreeSet::new();
    let mut unique_chunk_sizes: BTreeMap<String, u64> = BTreeMap::new();
    let mut selected_chunks = 0usize;
    for skill in &selected {
        let skill_record = load_skill_record(&source_db, &manifest, skill)?;
        let chunks = load_skill_chunks(&source_db, skill)?;
        selected_chunks += chunks.len();
        for chunk in &chunks {
            let stored_chunk = store.encode_and_put_chunk(&chunk.data, CompressionCodec::Gzip)?;
            if unique_chunk_hashes.insert(stored_chunk.hash.clone()) {
                unique_chunk_sizes.insert(
                    stored_chunk.hash.clone(),
                    stored_chunk.compressed_len as u64,
                );
            }
        }
        store.put_skill_chunks(
            &skill_record.id,
            &chunks
                .iter()
                .enumerate()
                .map(|(ordinal, chunk)| SkillChunkLink {
                    ordinal: ordinal as u32,
                    chunk_hash: chunk.hash.clone(),
                })
                .collect::<Vec<_>>(),
        )?;
        store.put_skill(&skill_record)?;
        for term in skill_record.index_terms() {
            store.add_term(&term, &skill_record.id)?;
        }
        for tag in &skill_record.tags {
            store.add_tag(tag, &skill_record.id)?;
        }
        for alias in &skill_record.aliases {
            store.set_alias(alias, &skill_record.id)?;
        }
        store.record_feedback(&skill_record.id, true, "ingested", None)?;
    }

    let stored_bytes = unique_chunk_sizes.values().sum();
    let corpus_hash = {
        let mut hasher = Sha256::new();
        for skill in &manifest.skills {
            hasher.update(skill.fingerprint.as_bytes());
            hasher.update([0]);
            hasher.update(skill.skill_id.as_bytes());
            hasher.update([0]);
        }
        hex::encode(hasher.finalize())
    };

    let meta = SkillPackMeta::new(
        corpus_hash,
        manifest.corpus_root.clone(),
        CompressionCodec::Gzip,
        selected.len() as u64,
        unique_chunk_hashes.len() as u64,
    );
    store.put_pack_meta(&meta)?;

    Ok(SkillsPackReport {
        source_manifest: config.manifest_path,
        source_db: config.source_db,
        output: config.output,
        selection_manifest,
        selected_tiers: if uses_curated_manifest {
            Vec::new()
        } else {
            selected_tiers
        },
        selected_skills: selected.len(),
        selected_chunks,
        stored_bytes,
        corpus_bytes: manifest.summary.total_bytes,
    })
}

pub fn install_pack(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> anyhow::Result<SkillsInstallReport> {
    let source = source.as_ref().to_path_buf();
    let destination = destination.as_ref().to_path_buf();
    if !source.exists() {
        anyhow::bail!("bundled skills pack {} does not exist", source.display());
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let bytes_copied = std::fs::copy(&source, &destination)
        .with_context(|| format!("copying {} to {}", source.display(), destination.display()))?;
    Ok(SkillsInstallReport {
        source,
        destination,
        bytes_copied,
    })
}

fn build_report(mut manifest: SkillsAuditManifest) -> SkillsAuditReport {
    let mut cluster_sizes: BTreeMap<String, usize> = BTreeMap::new();
    for skill in &manifest.skills {
        *cluster_sizes.entry(skill.fingerprint.clone()).or_insert(0) += 1;
    }
    for skill in &mut manifest.skills {
        skill.duplicate_cluster_size = cluster_sizes.get(&skill.fingerprint).copied().unwrap_or(1);
    }

    manifest.skills.sort_by(|a, b| {
        b.estimated_score
            .cmp(&a.estimated_score)
            .then(a.skill_id.cmp(&b.skill_id))
    });

    let top_skills = manifest.skills.iter().take(20).cloned().collect::<Vec<_>>();
    manifest.skills.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));

    SkillsAuditReport {
        manifest,
        top_skills,
    }
}

fn build_manifest(corpus_root: &Path, source_db: &Path) -> anyhow::Result<SkillsAuditManifest> {
    let conn = Connection::open_with_flags(source_db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", source_db.display()))?;

    let mut aggregate_stmt = conn.prepare(
        r#"
        SELECT
            s.id,
            s.source,
            s.skill_id,
            COALESCE(s.name, ''),
            COALESCE(s.installs, 0),
            s.page_url,
            s.repo_url,
            COALESCE(s.status, ''),
            COUNT(f.id) AS file_count,
            COALESCE(SUM(CASE WHEN f.is_markdown = 1 THEN 1 ELSE 0 END), 0) AS markdown_files,
            COALESCE(SUM(CASE WHEN f.is_script = 1 THEN 1 ELSE 0 END), 0) AS script_files,
            COALESCE(SUM(COALESCE(f.size, length(f.content), 0)), 0) AS total_bytes,
            COALESCE(MAX(CASE WHEN f.path = 'SKILL.md' THEN 1 ELSE 0 END), 0) AS has_skill_md
        FROM skills s
        LEFT JOIN skill_files f ON f.skill_id = s.id
        GROUP BY s.id
        ORDER BY s.skill_id
        "#,
    )?;

    let mut skills = aggregate_stmt
        .query_map(params![], |row| {
            let db_id: i64 = row.get(0)?;
            let source: String = row.get(1)?;
            let skill_id: String = row.get(2)?;
            let name: String = row.get(3)?;
            let installs: i64 = row.get(4)?;
            let page_url: Option<String> = row.get(5)?;
            let repo_url: Option<String> = row.get(6)?;
            let status: String = row.get(7)?;
            let file_count: i64 = row.get(8)?;
            let markdown_files: i64 = row.get(9)?;
            let script_files: i64 = row.get(10)?;
            let total_bytes: i64 = row.get(11)?;
            let has_skill_md: i64 = row.get(12)?;
            let file_count = file_count.max(0) as usize;
            let markdown_files = markdown_files.max(0) as usize;
            let script_files = script_files.max(0) as usize;
            let total_bytes = total_bytes.max(0) as u64;
            let has_skill_md = has_skill_md != 0;
            let estimated_score = estimate_score(
                installs,
                file_count,
                markdown_files,
                script_files,
                has_skill_md,
            );
            let (suggested_tier, suggested_reason) = suggest_tier(
                &status,
                installs,
                file_count,
                markdown_files,
                script_files,
                has_skill_md,
                estimated_score,
            );

            Ok(SkillAuditRecord {
                db_id,
                source,
                skill_id,
                name,
                installs,
                page_url,
                repo_url,
                status,
                file_count,
                markdown_files,
                script_files,
                total_bytes,
                has_skill_md,
                canonical_file: if has_skill_md {
                    Some("SKILL.md".to_string())
                } else {
                    None
                },
                fingerprint: String::new(),
                duplicate_cluster_size: 1,
                estimated_score,
                suggested_tier,
                suggested_reason,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut fingerprint_stmt = conn.prepare(
        r#"
        SELECT
            s.id,
            f.path,
            f.sha,
            f.size,
            f.is_markdown,
            f.is_script
        FROM skills s
        LEFT JOIN skill_files f ON f.skill_id = s.id
        ORDER BY s.id, COALESCE(f.path, '')
        "#,
    )?;

    let mut fingerprints: BTreeMap<i64, (String, Option<String>)> = BTreeMap::new();
    let mut rows = fingerprint_stmt.query(params![])?;
    let mut current_db_id: Option<i64> = None;
    let mut hasher = Sha256::new();
    let mut canonical_file: Option<String> = None;

    while let Some(row) = rows.next()? {
        let db_id: i64 = row.get(0)?;
        let path: Option<String> = row.get(1)?;
        let sha: Option<String> = row.get(2)?;
        let size: Option<i64> = row.get(3)?;
        let is_markdown: Option<i64> = row.get(4)?;
        let is_script: Option<i64> = row.get(5)?;

        if current_db_id != Some(db_id) {
            if let Some(previous_db_id) = current_db_id.take() {
                fingerprints.insert(
                    previous_db_id,
                    (hex::encode(hasher.finalize_reset()), canonical_file.take()),
                );
            }
            current_db_id = Some(db_id);
            hasher.update(db_id.to_le_bytes());
            canonical_file = None;
        }

        if let Some(path) = path {
            hasher.update(path.as_bytes());
            hasher.update([0]);
            if canonical_file.is_none() && (path == "SKILL.md" || path.ends_with(".md")) {
                canonical_file = Some(path.clone());
            }
            match sha {
                Some(sha) if !sha.is_empty() => hasher.update(sha.as_bytes()),
                _ => hasher.update(size.unwrap_or_default().to_le_bytes()),
            }
            hasher.update([0]);
            hasher.update(size.unwrap_or_default().to_le_bytes());
            hasher.update([0]);
            hasher.update([u8::from(is_markdown.unwrap_or_default() != 0)]);
            hasher.update([u8::from(is_script.unwrap_or_default() != 0)]);
        }
    }

    if let Some(previous_db_id) = current_db_id.take() {
        fingerprints.insert(
            previous_db_id,
            (hex::encode(hasher.finalize_reset()), canonical_file.take()),
        );
    }

    for skill in &mut skills {
        if let Some((fingerprint, canonical)) = fingerprints.get(&skill.db_id) {
            skill.fingerprint = fingerprint.clone();
            if skill.canonical_file.is_none() {
                skill.canonical_file = canonical.clone();
            }
        }
    }

    let mut summary = SkillsAuditSummary {
        total_skills: skills.len(),
        total_files: skills.iter().map(|skill| skill.file_count).sum(),
        total_bytes: skills.iter().map(|skill| skill.total_bytes).sum(),
        duplicate_clusters: 0,
        core_candidates: 0,
        domain_candidates: 0,
        archive_candidates: 0,
        excluded_candidates: 0,
    };

    for skill in &skills {
        match skill.suggested_tier {
            SkillTier::Core => summary.core_candidates += 1,
            SkillTier::Domain => summary.domain_candidates += 1,
            SkillTier::Archive => summary.archive_candidates += 1,
            SkillTier::Exclude => summary.excluded_candidates += 1,
        }
    }

    let mut cluster_sizes: BTreeMap<&str, usize> = BTreeMap::new();
    for skill in &skills {
        *cluster_sizes.entry(skill.fingerprint.as_str()).or_insert(0) += 1;
    }
    summary.duplicate_clusters = cluster_sizes.values().filter(|count| **count > 1).count();

    let generated_at = rfc3339_now();
    Ok(SkillsAuditManifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        source_db: source_db.to_string_lossy().into_owned(),
        corpus_root: corpus_root.to_string_lossy().into_owned(),
        generated_at,
        summary,
        skills,
    })
}

fn render_markdown(report: &SkillsAuditReport) -> String {
    let mut out = String::new();
    out.push_str("# Skills Audit\n\n");
    out.push_str(&format!("- Source DB: `{}`\n", report.manifest.source_db));
    out.push_str(&format!(
        "- Corpus root: `{}`\n",
        report.manifest.corpus_root
    ));
    out.push_str(&format!(
        "- Total skills: `{}`\n",
        report.manifest.summary.total_skills
    ));
    out.push_str(&format!(
        "- Core candidates: `{}`\n",
        report.manifest.summary.core_candidates
    ));
    out.push_str("\n## Top Skills\n\n");
    for skill in &report.top_skills {
        out.push_str(&format!(
            "- `{}` — `{}` — `{:?}` — score `{}`\n",
            skill.skill_id, skill.name, skill.suggested_tier, skill.estimated_score
        ));
    }
    out
}

fn estimate_score(
    installs: i64,
    file_count: usize,
    markdown_files: usize,
    script_files: usize,
    has_skill_md: bool,
) -> i64 {
    let install_score = installs.max(0) / 10;
    let file_score = (file_count as i64) * 60;
    let markdown_score = (markdown_files as i64) * 120;
    let script_score = (script_files as i64) * 50;
    let skill_md_score = if has_skill_md { 250 } else { 0 };
    install_score + file_score + markdown_score + script_score + skill_md_score
}

fn suggest_tier(
    status: &str,
    installs: i64,
    file_count: usize,
    markdown_files: usize,
    script_files: usize,
    has_skill_md: bool,
    score: i64,
) -> (SkillTier, String) {
    if status.eq_ignore_ascii_case("error") || status.eq_ignore_ascii_case("failed") {
        return (
            SkillTier::Exclude,
            "corpus status marks this skill as failed".to_string(),
        );
    }

    if has_skill_md && installs >= 5_000 {
        return (
            SkillTier::Core,
            "high install count with a canonical SKILL.md".to_string(),
        );
    }

    if has_skill_md && (score >= 700 || file_count >= 1 || markdown_files >= 1 || script_files >= 1)
    {
        return (
            SkillTier::Domain,
            "has a canonical skill document and enough signal to keep in the default pack"
                .to_string(),
        );
    }

    if score >= 400 {
        return (
            SkillTier::Domain,
            "signal score is strong enough for a non-core pack".to_string(),
        );
    }

    (
        SkillTier::Archive,
        "low signal or incomplete structure; keep out of the default pack".to_string(),
    )
}

fn rfc3339_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn select_manifest_skills(
    manifest: &SkillsAuditManifest,
    tiers: &[SkillsPackTier],
) -> Vec<SkillAuditRecord> {
    let allowed: BTreeSet<SkillsPackTier> = tiers.iter().copied().collect();
    manifest
        .skills
        .iter()
        .filter(|skill| match skill.suggested_tier {
            SkillTier::Core => allowed.contains(&SkillsPackTier::Core),
            SkillTier::Domain => allowed.contains(&SkillsPackTier::Domain),
            SkillTier::Archive => allowed.contains(&SkillsPackTier::Archive),
            SkillTier::Exclude => false,
        })
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CuratedSelectionManifest {
    version: String,
    name: String,
    description: String,
    selected_skill_ids: Vec<String>,
}

fn load_selection_manifest(path: &Path) -> anyhow::Result<CuratedSelectionManifest> {
    let manifest_text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        let manifest: CuratedSelectionManifest = serde_json::from_str(&manifest_text)
            .with_context(|| format!("parsing {}", path.display()))?;
        return Ok(manifest);
    }

    let mut selected_skill_ids = Vec::new();
    for line in manifest_text.lines() {
        let trimmed = line.trim_start();
        let Some((prefix, rest)) = trimmed.split_once(". ") else {
            continue;
        };
        if !prefix.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Some(start) = rest.find('`') {
            let after = &rest[start + 1..];
            if let Some(end) = after.find('`') {
                selected_skill_ids.push(after[..end].to_string());
            }
        }
    }
    if selected_skill_ids.is_empty() {
        anyhow::bail!(
            "no selected skill ids found in curated selection manifest {}",
            path.display()
        );
    }
    Ok(CuratedSelectionManifest {
        version: "1".to_string(),
        name: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("curated-selection")
            .to_string(),
        description: format!("curated selection loaded from {}", path.display()),
        selected_skill_ids,
    })
}

fn select_manifest_skills_by_ids(
    manifest: &SkillsAuditManifest,
    selected_skill_ids: &[String],
) -> anyhow::Result<Vec<SkillAuditRecord>> {
    let by_skill_id: BTreeMap<&str, &SkillAuditRecord> = manifest
        .skills
        .iter()
        .map(|skill| (skill.skill_id.as_str(), skill))
        .collect();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for skill_id in selected_skill_ids {
        if !seen.insert(skill_id.clone()) {
            continue;
        }
        let skill = by_skill_id.get(skill_id.as_str()).with_context(|| {
            format!(
                "selected skill id {} is missing from the audit manifest",
                skill_id
            )
        })?;
        if matches!(skill.suggested_tier, SkillTier::Exclude) {
            anyhow::bail!(
                "selected skill id {} is marked exclude in the audit manifest",
                skill_id
            );
        }
        selected.push((*skill).clone());
    }
    Ok(selected)
}

fn load_skill_record(
    conn: &Connection,
    manifest: &SkillsAuditManifest,
    skill: &SkillAuditRecord,
) -> anyhow::Result<SkillRecord> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            COALESCE(f.path, ''),
            COALESCE(f.content, ''),
            COALESCE(f.is_markdown, 0),
            COALESCE(f.is_script, 0),
            COALESCE(f.sha, ''),
            COALESCE(f.size, length(f.content), 0)
        FROM skill_files f
        WHERE f.skill_id = ?1
        ORDER BY CASE WHEN f.path = 'SKILL.md' THEN 0 ELSE 1 END, f.path
        "#,
    )?;
    let mut rows = stmt.query(params![skill.db_id])?;
    let mut keywords = BTreeSet::new();
    let mut prerequisites = Vec::new();
    let mut validation = Vec::new();
    let mut summary = None;
    let source_root = manifest.corpus_root.clone();
    let mut source_path = skill
        .canonical_file
        .clone()
        .unwrap_or_else(|| skill.skill_id.clone());

    while let Some(row) = rows.next()? {
        let path: String = row.get(0)?;
        let content: String = row.get(1)?;
        let is_markdown: i64 = row.get(2)?;

        if summary.is_none() && is_markdown != 0 && !content.trim().is_empty() {
            summary = Some(first_paragraph(&content));
        }
        if keywords.is_empty() {
            for token in content
                .split(|c: char| !c.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .map(|token| token.to_ascii_lowercase())
                .filter(|token| token.len() > 2)
                .take(24)
            {
                keywords.insert(token);
            }
        }
        if is_markdown != 0 {
            if path == "SKILL.md" {
                source_path = path.clone();
            }
        }
    }

    let mut tags = skill_tags(skill, &keywords);
    tags.sort();
    tags.dedup();

    if prerequisites.is_empty() {
        prerequisites.push(format!("use for {}", skill.suggested_reason));
    }
    if validation.is_empty() {
        validation.push("verify the result matches the skill intent".to_string());
    }

    let record = SkillRecord {
        id: skill.skill_id.clone(),
        name: if skill.name.is_empty() {
            skill.skill_id.clone()
        } else {
            skill.name.clone()
        },
        version: "1.0.0".to_string(),
        purpose: summary
            .clone()
            .unwrap_or_else(|| format!("Use for {}", skill.suggested_reason)),
        tags,
        aliases: vec![skill.skill_id.clone()],
        prerequisites,
        safety_level: match skill.suggested_tier {
            SkillTier::Core | SkillTier::Domain => "advisory".to_string(),
            SkillTier::Archive => "reference".to_string(),
            SkillTier::Exclude => "excluded".to_string(),
        },
        validation,
        summary,
        source: SkillSource {
            source_root,
            source_path,
            corpus_hash: skill.fingerprint.clone(),
        },
        created_at: 0,
        updated_at: 0,
    };

    Ok(record)
}

fn skill_tags(skill: &SkillAuditRecord, keywords: &BTreeSet<String>) -> Vec<String> {
    let mut tags = vec![
        skill.suggested_tier.to_string(),
        skill
            .source
            .split('/')
            .next()
            .unwrap_or("unknown")
            .to_string(),
    ];
    tags.extend(keywords.iter().take(8).cloned());
    tags
}

fn first_paragraph(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .find(|paragraph| !paragraph.is_empty())
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn load_skill_chunks(
    conn: &Connection,
    skill: &SkillAuditRecord,
) -> anyhow::Result<Vec<SkillChunkRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            COALESCE(f.path, ''),
            COALESCE(f.content, ''),
            COALESCE(f.is_markdown, 0)
        FROM skill_files f
        WHERE f.skill_id = ?1
          AND COALESCE(f.is_markdown, 0) != 0
        ORDER BY CASE WHEN f.path = 'SKILL.md' THEN 0 ELSE 1 END, f.path
        LIMIT 1
        "#,
    )?;
    let mut rows = stmt.query(params![skill.db_id])?;
    let mut out = Vec::new();
    let mut primary = None;
    if let Some(row) = rows.next()? {
        let content: String = row.get(1)?;
        if !content.trim().is_empty() {
            primary = Some(content);
        }
    }

    if primary.is_none() {
        let mut fallback_stmt = conn.prepare(
            r#"
            SELECT COALESCE(f.content, '')
            FROM skill_files f
            WHERE f.skill_id = ?1
            ORDER BY CASE WHEN f.path = 'SKILL.md' THEN 0 ELSE 1 END, f.path
            LIMIT 1
            "#,
        )?;
        let mut fallback_rows = fallback_stmt.query(params![skill.db_id])?;
        if let Some(row) = fallback_rows.next()? {
            let content: String = row.get(0)?;
            if !content.trim().is_empty() {
                primary = Some(content);
            }
        }
    }

    if let Some(content) = primary {
        for chunk in chunk_text(&content) {
            let bytes = chunk.into_bytes();
            let hash = hash_bytes(&bytes);
            out.push(SkillChunkRecord::new(
                hash,
                CompressionCodec::Gzip,
                bytes.len(),
                bytes.len(),
                bytes,
            ));
        }
    }
    Ok(dedup_chunks(out))
}

fn chunk_text(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            if !current.is_empty() {
                chunks.push(current.join("\n"));
                current.clear();
            }
            continue;
        }
        current.push(trimmed.to_string());
    }
    if !current.is_empty() {
        chunks.push(current.join("\n"));
    }
    if chunks.is_empty() {
        chunks.push(text.trim().to_string());
    }
    chunks
}

fn dedup_chunks(chunks: Vec<SkillChunkRecord>) -> Vec<SkillChunkRecord> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for chunk in chunks {
        if seen.insert(chunk.hash.clone()) {
            out.push(chunk);
        }
    }
    out
}

pub fn print_audit_summary(report: &SkillsAuditReport) {
    println!("skills audit complete");
    println!("source db: {}", report.manifest.source_db);
    println!("corpus root: {}", report.manifest.corpus_root);
    println!("total skills: {}", report.manifest.summary.total_skills);
    println!("total files: {}", report.manifest.summary.total_files);
    println!("total bytes: {}", report.manifest.summary.total_bytes);
    println!(
        "duplicate clusters: {}",
        report.manifest.summary.duplicate_clusters
    );
    println!(
        "core candidates: {}",
        report.manifest.summary.core_candidates
    );
    println!(
        "domain candidates: {}",
        report.manifest.summary.domain_candidates
    );
    println!(
        "archive candidates: {}",
        report.manifest.summary.archive_candidates
    );
    println!(
        "excluded candidates: {}",
        report.manifest.summary.excluded_candidates
    );
    println!("top candidates:");
    for skill in report.top_skills.iter().take(10) {
        println!(
            "- {} [{:?}] score={} files={} installs={}",
            skill.skill_id,
            skill.suggested_tier,
            skill.estimated_score,
            skill.file_count,
            skill.installs
        );
    }
}

pub fn print_pack_summary(report: &SkillsPackReport) {
    println!("skills pack build complete");
    println!("source manifest: {}", report.source_manifest.display());
    println!("source db: {}", report.source_db.display());
    println!("output: {}", report.output.display());
    if let Some(selection_manifest) = &report.selection_manifest {
        println!("selection manifest: {}", selection_manifest.display());
    }
    println!("selected tiers: {:?}", report.selected_tiers);
    println!("selected skills: {}", report.selected_skills);
    println!("selected chunks: {}", report.selected_chunks);
    println!("stored bytes: {}", report.stored_bytes);
    println!("corpus bytes: {}", report.corpus_bytes);
}

pub fn print_install_summary(report: &SkillsInstallReport) {
    println!("skills pack install complete");
    println!("source: {}", report.source.display());
    println!("destination: {}", report.destination.display());
    println!("bytes copied: {}", report.bytes_copied);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn seed_db(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE skills (
                id INTEGER PRIMARY KEY,
                source TEXT NOT NULL,
                skill_id TEXT NOT NULL,
                name TEXT,
                installs INTEGER,
                page_url TEXT,
                repo_url TEXT,
                last_seen TEXT,
                status TEXT DEFAULT 'pending',
                error TEXT
            );
            CREATE TABLE skill_files (
                id INTEGER PRIMARY KEY,
                skill_id INTEGER NOT NULL,
                path TEXT NOT NULL,
                content TEXT,
                is_markdown INTEGER DEFAULT 0,
                is_script INTEGER DEFAULT 0,
                sha TEXT,
                size INTEGER
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO skills (id, source, skill_id, name, installs, page_url, repo_url, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![1, "demo/source", "demo-skill", "Demo Skill", 6000, "https://example.com", "https://repo.example.com", "skipped"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO skill_files (skill_id, path, content, is_markdown, is_script, sha, size) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![1, "SKILL.md", "# demo", 1, 0, "deadbeef", 7],
        )
        .unwrap();
    }

    #[test]
    fn audit_builds_manifest_and_suggests_core_for_high_signal_skill() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("skills.db");
        seed_db(&db);

        let report = build_manifest(Path::new("/corp"), &db).unwrap();
        assert_eq!(report.summary.total_skills, 1);
        assert_eq!(report.skills[0].skill_id, "demo-skill");
        assert_eq!(report.skills[0].suggested_tier, SkillTier::Core);
        assert!(report.skills[0].has_skill_md);
    }

    #[test]
    fn build_pack_writes_redb_from_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("skills.db");
        seed_db(&db);

        let manifest = build_manifest(Path::new("/corp"), &db).unwrap();
        let report = build_report(manifest);
        let manifest_path = dir.path().join("skills-audit.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&report.manifest).unwrap(),
        )
        .unwrap();
        let output = dir.path().join("skills.redb");

        let build = build_pack(PackBuildConfig {
            manifest_path: manifest_path.clone(),
            source_db: db.clone(),
            output: output.clone(),
            selection_manifest: None,
            selected_tiers: vec![SkillsPackTier::Core],
        })
        .unwrap();

        assert_eq!(build.selected_skills, 1);
        assert!(output.exists());

        let store = SkillStore::open(&output).unwrap();
        let meta = store.get_pack_meta().unwrap().unwrap();
        assert_eq!(meta.skill_count, 1);
        assert_eq!(store.list_skills().unwrap().len(), 1);
    }

    #[test]
    fn build_pack_uses_curated_selection_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("skills.db");
        seed_db(&db);

        let manifest = build_manifest(Path::new("/corp"), &db).unwrap();
        let report = build_report(manifest);
        let manifest_path = dir.path().join("skills-audit.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&report.manifest).unwrap(),
        )
        .unwrap();
        let curated_path = dir.path().join("selected-300-ruvos.json");
        std::fs::write(
            &curated_path,
            serde_json::json!({
                "version": "1",
                "name": "curated",
                "description": "curated selection",
                "selected_skill_ids": ["demo-skill"]
            })
            .to_string(),
        )
        .unwrap();
        let output = dir.path().join("skills.redb");

        let build = build_pack(PackBuildConfig {
            manifest_path: manifest_path.clone(),
            source_db: db.clone(),
            output: output.clone(),
            selection_manifest: Some(curated_path.clone()),
            selected_tiers: vec![SkillsPackTier::Archive],
        })
        .unwrap();

        assert_eq!(build.selected_skills, 1);
        assert_eq!(build.selection_manifest.as_ref(), Some(&curated_path));
        let store = SkillStore::open(&output).unwrap();
        assert_eq!(store.list_skills().unwrap().len(), 1);
    }

    #[test]
    fn install_pack_copies_bundled_pack_to_destination() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("skills.redb");
        let destination = dir.path().join("nested").join("skills.redb");
        std::fs::write(&source, b"pack-bytes").unwrap();

        let report = install_pack(&source, &destination).unwrap();

        assert_eq!(report.source, source);
        assert_eq!(report.destination, destination);
        assert_eq!(report.bytes_copied, 10);
        assert_eq!(std::fs::read(&destination).unwrap(), b"pack-bytes");
    }
}

# Skills Pack on redb

Ruvos should keep the skills corpus in a separate portable redb pack, not in the
live swarm state store.

## File layout

- `store.redb` — live runtime state
- `skills.redb` — normalized skills pack

## Tables

- `pack_meta`
  - corpus hash, schema version, source root, codec, build time, counts
- `skills`
  - skill metadata keyed by skill id
- `chunks`
  - deduplicated compressed chunk payloads keyed by content hash
- `skill_chunks`
  - ordered chunk references for each skill
- `tag_index`
  - tag to skill id postings
- `alias_index`
  - alias to canonical skill id
- `term_index`
  - lexical retrieval postings
- `source_index`
  - source/provenance postings
- `feedback`
  - selection and outcome counters per skill

## Build strategy

1. Run `ruvos skills audit` against the source corpus.
2. Use the generated audit manifest as the deterministic input for the pack builder.
3. Use the curated public selection manifest at `docs/skills/selected-300-ruvos.json` for the default shipped pack.
4. Run `ruvos skills pack build` with the audit manifest, curated selection manifest, and source SQLite DB.
5. Normalize each selected skill into canonical metadata and chunked text.
6. Hash each chunk by content.
7. Store each unique chunk once in `chunks`.
8. Store the ordered chunk list in `skill_chunks`.
9. Update tag, alias, term, and source indexes.
10. Record build metadata in `pack_meta`.

## Selection policy

- The public default pack is the curated 300-skill selection in `docs/skills/selected-300-ruvos.json`.
- `core`, `domain`, and `archive` candidates remain available for alternate pack builds.
- `exclude` candidates are skipped from pack generation entirely.

## Default pack

The default public pack is the curated 300-skill selection. It is derived from
the audit manifest, deduped by `skill_id`, and built into `skills.redb` using
the curated selection manifest.

## Runtime strategy

- Open the pack only for the duration of the query.
- Keep retrieval lexical first.
- `orchestrate.run` selects one skill bundle per run using the full orchestration plan and passes it to every spawned step.
- `agent.spawn` uses a provided bundle when present; otherwise it auto-selects up to 3 relevant skills from `skills.redb`.
- Each orchestration writes the chosen bundle to `generated/<orchestration_id>/selected-skills.json`.
- Each completed run records success/failure feedback back into `skills.redb`.
- Use feedback to rank the best skills over time.

# Skills Audit

`ruvos skills audit` scans the source skills corpus SQLite database and emits a
deterministic manifest that can drive the redb pack builder.

## Default paths

- Corpus root: `/mnt/datadisk/dev/skillbase`
- Source database: `/mnt/datadisk/dev/skillbase/data/skills.db`
- Default output: `generated/skills-audit.json`
- Curated public selection: `docs/skills/selected-300-ruvos.json`

## Outputs

- total skill count
- total file count
- byte total
- duplicate cluster count
- suggested tier per skill
- top scored skills

## Usage

```bash
ruvos skills audit --write generated/skills-audit.json
```

Then build the pack:

```bash
ruvos skills pack build --manifest generated/skills-audit.json --selection-manifest docs/skills/selected-300-ruvos.json --output generated/skills.redb
```

Then install the bundled pack into the runtime data directory:

```bash
ruvos skills pack install --source docs/skills/public/skills.redb
```

The bundled pack lives at `docs/skills/public/skills.redb` in the repository.

## Selection tiers

- `core` — optional inclusion for alternate pack builds
- `domain` — include in optional extension packs
- `archive` — keep in the manifest, but not in the default pack
- `exclude` — skip from pack generation

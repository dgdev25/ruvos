# Contract Manifest

`docs/contracts/contract-manifest.json` is the canonical machine-readable
contract for the live rUvOS tool registry, archetype vocabulary, and hook
model.

Use the CLI to keep it in sync:

```bash
cargo run -p ruvos-cli -- contracts generate --write docs/contracts/contract-manifest.json
cargo run -p ruvos-cli -- contracts check docs/contracts/contract-manifest.json
```

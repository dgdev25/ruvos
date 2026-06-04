# Contributing to rUvOS

Use the repo workflow rather than ad hoc commands:

- `just ci` for the full local gate.
- `just doctor` to inspect live invariants.
- `just contracts-check` to verify the canonical contract manifest.

Before sending a change:

- Keep the workspace green with `cargo test --workspace --jobs 4`.
- Regenerate the manifest with `just contracts-generate` if the live registry changes.
- Update the relevant ADR validation notes when the workflow contract changes.

The canonical contract manifest lives at `docs/contracts/contract-manifest.json`.

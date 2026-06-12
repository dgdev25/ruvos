# ADR-040: Verified Plugin Tarball Installation (`ruvos plugin install`)

**Status:** Accepted
**Date:** 2026-06-12

## Context

The scope ledger promises a plugin acquisition story — signed plugin artifacts
installed into the canonical layout at `./.ruvos/plugins/<name>/` — but until
now no such story existed. Users hand-copied plugin directories into place.
There was no integrity check, no transport, and no convention for publishing a
plugin.

## Decision

Add `ruvos plugin install <name> --from <url-or-path>`:

- **Distribution format:** `<name>.tar.gz` containing the canonical plugin
  layout (`plugin.toml` at the archive root, plus `agents/`, `skills/`,
  `commands/`), accompanied by sidecar files next to the tarball:
  - `<name>.tar.gz.sha256` — **required** hex SHA-256 digest of the tarball
    bytes. Missing sidecar is a hard install error.
  - `<name>.tar.gz.sig` — **optional** HMAC-SHA256 over the tarball bytes,
    keyed by `RUVOS_PLUGIN_KEY` (the same shared-key primitive `rvf-crypto`
    already uses for `.rvf` signing). If a `.sig` exists but no key is
    configured, install proceeds with a loud warning that the signature was
    NOT verified.
- **Transport:** local filesystem paths are first-class (work offline and in
  tests); remote fetch is `reqwest` with rustls, **https-only** — plaintext
  `http://` is refused.
- **Unpack safety:** entries are extracted into a staging directory next to
  the destination; any entry with an absolute path or a `..` component is
  rejected before extraction. After validating `plugin.toml` exists at the
  archive root, the staging dir is atomically renamed to
  `./.ruvos/plugins/<name>/`. An existing plugin directory is never
  overwritten (remove it first).
- **Code placement:** verification + unpack live in
  `ruvos-plugin-host::install` (`install_tarball`); fetch + CLI command live
  in `ruvos-cli::commands::plugin`. Installed plugins are discoverable by
  `plugin.list` immediately — no registration step.

Publishing convention:

```bash
tar -czf demo.tar.gz -C plugin-dir . && sha256sum demo.tar.gz > demo.tar.gz.sha256
```

## Explicitly deferred

- **Registry / index discovery** (`ruvos plugin search`, named registries,
  version resolution). `--from` takes an explicit URL or path only.
- **Asymmetric signatures.** HMAC-SHA256 is a shared-key scheme — it proves
  the publisher and installer hold the same key, matching the existing `.rvf`
  signing primitive. The upgrade path to publisher-verifiable ed25519
  signatures via `rvf-crypto` lands here when a registry exists to distribute
  public keys.
- **Uninstall / upgrade subcommands.** Removal is `rm -rf
  .ruvos/plugins/<name>` for now; reinstall requires removing first.

## Consequences

- Plugins gain a one-line, integrity-checked install path that works offline
  and over https, closing the acquisition gap in the scope ledger.
- The required `.sha256` sidecar makes silent tarball corruption or tampering
  in transit detectable by default, even for unsigned plugins.
- Path-traversal-hostile archives cannot write outside the plugin directory;
  failed installs leave no partial state (staging dir cleanup + atomic
  rename).
- Shared-key HMAC provides only mutual-key authenticity; supply-chain-grade
  publisher verification waits on the deferred ed25519 work.
- New deps: `tar`, `flate2`, `sha2`, `hmac`, `hex` in `ruvos-plugin-host`;
  `reqwest` (rustls, no default features) in `ruvos-cli`.

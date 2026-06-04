set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just ci

build:
    cargo build --workspace --jobs 4

build-release:
    cargo build --release -p ruvos-cli

test:
    cargo test --workspace --jobs 4

fmt:
    cargo fmt --check

clippy:
    cargo clippy --workspace --all-targets --jobs 4 -- -D warnings

doctor:
    cargo run -p ruvos-cli -- doctor --strict

contracts-generate:
    cargo run -p ruvos-cli -- contracts generate --write docs/contracts/contract-manifest.json

contracts-check:
    cargo run -p ruvos-cli -- contracts check docs/contracts/contract-manifest.json

smoke:
    cargo test -p ruvos-mcp --test integration_test -- --nocapture

ci: fmt clippy test doctor contracts-check smoke

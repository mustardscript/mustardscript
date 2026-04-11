#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo test -p jslite --test security_hostile_inputs
cargo test -p jslite --test property_generated_execution
cargo test -p jslite --test property_roundtrip
cargo test -p jslite --test property_snapshot_roundtrip
cargo test -p jslite-sidecar --test hostile_protocol
cargo check --manifest-path fuzz/Cargo.toml --bins

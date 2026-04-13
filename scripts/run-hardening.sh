#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fuzz_seconds="${MUSTARD_FUZZ_SECONDS:-10}"
fuzz_targets="${MUSTARD_FUZZ_TARGETS:-parser snapshot_load sidecar_protocol}"
fuzz_toolchain="${MUSTARD_FUZZ_TOOLCHAIN:-nightly}"
fuzz_artifact_root="${MUSTARD_FUZZ_ARTIFACT_ROOT:-fuzz/artifacts}"

cargo test -p mustard --test security_hostile_inputs
cargo test -p mustard --test property_generated_execution
cargo test -p mustard --test property_roundtrip
cargo test -p mustard --test property_snapshot_roundtrip
cargo test -p mustard-sidecar --test hostile_protocol

npm run build
node scripts/seed-fuzz-corpus.ts

if ! cargo fuzz --help >/dev/null 2>&1; then
  cargo install cargo-fuzz --locked
fi

if ! rustup toolchain list | grep -q "^${fuzz_toolchain}"; then
  rustup toolchain install "${fuzz_toolchain}" --profile minimal
fi

mkdir -p "${fuzz_artifact_root}"
for target in ${fuzz_targets}; do
  target_artifact_dir="${fuzz_artifact_root}/${target}"
  mkdir -p "${target_artifact_dir}"
  cargo +"${fuzz_toolchain}" fuzz run "${target}" -- \
    "-max_total_time=${fuzz_seconds}" \
    "-print_final_stats=1" \
    "-verbosity=0"
done

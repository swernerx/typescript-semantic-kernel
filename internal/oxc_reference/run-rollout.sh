#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
rollout_tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/ts7-rust-rollout.XXXXXX")"
trap 'rm -rf -- "$rollout_tmpdir"' EXIT

output_path=""
if [[ $# -gt 0 ]]; then
    if [[ $# -ne 2 || $1 != "--output" ]]; then
        echo "usage: $0 [--output <path>]" >&2
        exit 2
    fi
    output_path="$2"
fi

cd "$repository_root"
repository_revision="$(git rev-parse HEAD)"
GOCACHE="$repository_root/internal/oxc_reference/target/go-build-cache" \
    go build -trimpath -o "$rollout_tmpdir/tsfacts" ./cmd/tsfacts
cargo build --quiet --release --locked \
    --manifest-path internal/oxc_reference/Cargo.toml \
    --bin oxc-occurrence-map

rollout_command=(
    "$repository_root/internal/oxc_reference/target/release/oxc-occurrence-map"
    rollout
    "$rollout_tmpdir/tsfacts"
    "$repository_root/internal/semanticfacts/testdata/corpus/v0"
    "$repository_revision"
)

if [[ -n "$output_path" ]]; then
    "${rollout_command[@]}" --output "$output_path"
else
    "${rollout_command[@]}"
fi

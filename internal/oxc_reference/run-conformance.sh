#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
conformance_tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/ts7-rust-conformance.XXXXXX")"
trap 'rm -rf -- "$conformance_tmpdir"' EXIT

output_path=""
if [[ $# -gt 0 ]]; then
    if [[ $# -ne 2 || $1 != "--output" ]]; then
        echo "usage: $0 [--output <path>]" >&2
        exit 2
    fi
    output_path="$2"
fi

cd "$repository_root"
GOCACHE="$repository_root/internal/oxc_reference/target/go-build-cache" \
    go build -o "$conformance_tmpdir/tsfacts" ./cmd/tsfacts
cargo run --quiet --locked \
    --manifest-path internal/oxc_reference/Cargo.toml \
    --bin oxc-occurrence-map -- \
    conformance "$conformance_tmpdir/tsfacts" \
    internal/semanticfacts/testdata/corpus/v0 \
    --output "$conformance_tmpdir/report-first.json"
cargo run --quiet --locked \
    --manifest-path internal/oxc_reference/Cargo.toml \
    --bin oxc-occurrence-map -- \
    conformance "$conformance_tmpdir/tsfacts" \
    internal/semanticfacts/testdata/corpus/v0 \
    --output "$conformance_tmpdir/report-repeated.json"

if ! cmp -s "$conformance_tmpdir/report-first.json" "$conformance_tmpdir/report-repeated.json"; then
    echo "conformance reports differ across repeated full-corpus runs" >&2
    cmp "$conformance_tmpdir/report-first.json" "$conformance_tmpdir/report-repeated.json" || true
    exit 1
fi

if [[ -n "$output_path" ]]; then
    cp "$conformance_tmpdir/report-first.json" "$output_path"
else
    cat "$conformance_tmpdir/report-first.json"
fi

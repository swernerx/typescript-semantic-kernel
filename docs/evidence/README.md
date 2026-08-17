# Spike evidence

Evidence files in this directory are machine-readable records from bounded,
repository-local migration spikes. They are not general performance benchmarks
and do not transfer semantic authority away from the TypeScript 7 Go checker.

Regenerate the TS7-to-OXC/Rust record from the repository root with:

```sh
./internal/oxc_reference/run-evidence.sh \
  --output docs/evidence/ts7-oxc-spike-2026-08-17.json
```

The command builds the real `cmd/tsfacts` producer, runs every semantic corpus
case twice through the JSON Lines process boundary, decodes and attaches each
response in the internal OXC/Rust consumer, and performs bounded graph
inspection. Stable observations are compared byte-for-byte in memory; timing
and resident-memory values are recorded separately because they vary by host.

Diagnostics name their failure layer: `protocol`, `exporter`, `mapping`, or
`consumer`. Mapping outcomes distinguish unmapped and multiply-mapped facts.
Inspector counters retain unavailable roots, unsupported/error/truncated
entities, and independent depth/node/edge budget cutoffs even when a measured
count is zero.

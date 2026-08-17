# Internal OXC occurrence consumer

This isolated Cargo workspace is a reference and migration harness for the
portable occurrence-correlation contract in `docs/occurrence-correlation.md`.
It is not a second semantic authority: TypeScript 7's Go checker remains the
oracle for semantic facts.

The consumer parses representative TypeScript and TSX fixture sources with OXC,
builds OXC's arena-local semantic node table, projects relevant syntax nodes
onto schema-v1 spans and kind names, and correlates portable facts. Successful
matches remain a Rust-owned `fact index -> oxc_semantic::NodeId` side table for
the lifetime of the OXC arena. Only portable spans, kind names, numeric
response-local mapping IDs, diagnostics, and coverage are serialized. No OXC
or Rust type belongs in the TS7 producer protocol.

Run the focused tests and the machine-readable fixture report from the
repository root:

```sh
cargo test --locked --manifest-path internal/oxc_reference/Cargo.toml
cargo run --locked --manifest-path internal/oxc_reference/Cargo.toml \
  --bin oxc-occurrence-map -- fixtures
```

Inspect a single-file semantic-facts response against its source with:

```sh
cargo run --locked --manifest-path internal/oxc_reference/Cargo.toml \
  --bin oxc-occurrence-map -- inspect snapshot.jsonl source.ts src/source.ts
```

The optional final argument overrides the normalized logical file ID used by
the semantic response. A single-file snapshot otherwise supplies that ID from
its first fact.

Run the complete TS7-to-consumer evidence slice with one command:

```sh
./internal/oxc_reference/run-evidence.sh \
  --output docs/evidence/ts7-oxc-spike-2026-08-17.json
```

The script builds the real Go `tsfacts` binary and runs every semantic corpus
case twice. The Rust runner preserves response-global fact indices while
attaching each project file, compares stable non-timing observations, and emits
ordered JSON with producer graph counts/budgets, OXC mapping coverage,
inspection depth/node/edge use, artifact/timing measurements, and diagnostics
classified as protocol, exporter, mapping, or consumer failures. Timings are
first and immediately repeated one-shot measurements, not daemon benchmarks.

The checked evidence maps every fact from OXC-parseable sources but records the
intentional syntax-recovery source as a consumer failure. It does not infer
semantic equivalence from those mappings. See
[ADR-0016](../../docs/adr/0016-port-occurrence-attachment-before-semantic-categories.md).

Run the deterministic Go-versus-Rust shadow conformance gate with:

```sh
./internal/oxc_reference/run-conformance.sh \
  --output /tmp/ts7-rust-conformance.json
```

The command runs every explicitly classified primitive/literal corpus fixture
through the Go oracle and the version-2 Rust producer using the internal
version-2 candidate schema. Rust derives its records independently from the
project source and OXC semantic nodes; the Go graph is supplied only to the
comparator. Its JSON compares fact identity, exact OXC mapping, all five roots,
response-local graph identity through a Go-to-Rust ID bijection, structured
payloads, recovery, unsupported states, and truncation. Every selected fixture
contains a structured Go expectation and exactly one Rust classification:
`supported`, `unsupported`, `budget`, or `mapping`. The three recovery-file
facts remain individually visible as expected `oxc-parse-error` mapping gaps.
Unexplained semantic, transport, or mapping differences fail the command;
expected unsupported, budget, and mapping cases remain separately reported.
The wrapper executes the complete report twice and requires byte-identical
files. The gate is a shadow comparison and does not change the producer or
production routing. See
[ADR-0019](../../docs/adr/0019-compute-primitive-literals-independently-in-rust.md).

Run the controlled rollout evidence path with release binaries:

```sh
./internal/oxc_reference/run-rollout.sh \
  --output docs/evidence/primitive-literal-rollout-2026-08-17.json
```

This command executes two complete Go/Rust dual-runs over the same ordered
manifest requests and requires their embedded conformance reports to be
byte-identical. The report pins the repository and TypeScript revisions,
projects, capabilities, budgets, and selection counts. It also records two
host-dependent runtime/output-size samples, release artifact sizes, and the
Rust controller's explicitly scoped resident-memory measurement. Those
measurements characterize the one-shot Go versus in-process Rust harness; they
are not compatibility thresholds or a performance claim. Go remains the
serving authority and fallback, and Rust stays shadow-only. See
[ADR-0020](../../docs/adr/0020-keep-primitive-literals-shadow-only-after-dual-run.md).

The first test suite applies the Rust implementation of the portable contract
to every shared JSON fixture in `internal/occurrencemap/testdata/v1` and checks
its complete expected report. It separately parses those fixture sources with
OXC and verifies that emitted mappings resolve back to typed, arena-local
`NodeId`s. The command prints OXC-produced mappings, unmapped/ambiguity
diagnostics, and integer coverage counters grouped by semantic syntax kind.

## TypeFacts attachment and graph inspection

The consumer can decode schema-v1 JSON Lines into one shared, immutable
`TypeGraph`, correlate its occurrence facts, and index successful attachments
by typed OXC `NodeId`. The side table stores fact indices in response order, so
repeated selections are retained. Each attachment exposes effective actual,
contextual, widened, apparent, and declared roots; `same-as-actual` resolves to
the actual TypeID without hiding the view state.

Inspection output is pretty-printed deterministic JSON. Roots and edges retain
response-local IDs, each graph identity is emitted once, and recursive graphs
are not expanded into trees. Fact/entity completeness, issue codes,
unsupported and truncated states, unavailable views, and correlation
diagnostics remain visible. Depth, node, and edge budgets are consumer-local
guards and do not rewrite or reinterpret producer records. See
[ADR-0015](../../docs/adr/0015-attach-semantic-facts-without-expanding-graph-identity.md).

Graph inspection remains a consumer of the Go response and does not synthesize
Rust candidate records. The independent primitive/literal producer lives beside
the inspector and is exercised only by focused tests and the shadow conformance
runner. It accepts source/project inputs, owns its response-local type identity,
and never receives the Go graph used as the oracle.

## Migration boundary

Keep parser-specific traversal and allow-listed span/kind projection in this
workspace. Keep semantic-facts production, checker meaning, and protocol
negotiation in Go. The intended migration sequence is:

1. mechanically port linked Go categories where that preserves behavior;
2. compare each Rust category against the Go oracle and shared fixtures;
3. replace one category at a time only after its compatibility threshold is
   explicit and met.

Occurrence identity and attachment plumbing is the first approved mechanical
port category. Primitive/literal construction is now independently implemented
for the narrow, tagged conformance corpus and satisfies the shadow threshold in
ADR-0019. The controlled dual-run in ADR-0020 retains explicit unsupported,
recovery, serving-integration, measurement-boundary, and memory blockers. This
does not transfer semantic authority: the implementation is not wired into
production, Go remains the fallback, and broader TypeScript semantics remain
Go-authoritative.

Adding a projection does not transfer semantic authority. New parser-boundary
normalizations require a shared fixture and the versioned portable contract.

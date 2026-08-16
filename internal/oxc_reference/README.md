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

## Migration boundary

Keep parser-specific traversal and allow-listed span/kind projection in this
workspace. Keep semantic-facts production, checker meaning, and protocol
negotiation in Go. The intended migration sequence is:

1. mechanically port linked Go categories where that preserves behavior;
2. compare each Rust category against the Go oracle and shared fixtures;
3. replace one category at a time only after its compatibility threshold is
   explicit and met.

Adding a projection does not transfer semantic authority. New parser-boundary
normalizations require a shared fixture and the versioned portable contract.

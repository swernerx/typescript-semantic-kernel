# ADR-0016: Port occurrence attachment before semantic categories

- Status: accepted
- Date: 2026-08-17
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

RFC 0001 requires measured TS7-to-OXC/Rust evidence before selecting a Rust
migration category. ADR-0014 isolated live OXC traversal and ADR-0015 attached
the producer's response-local graph without copying identity. Neither result
compared the complete representative Go corpus through that internal boundary
or established a replacement gate.

Syntax correlation alone cannot demonstrate TypeScript semantic equivalence.
The OXC harness observes source identities and consumes Go-produced TypeFacts;
it does not independently reproduce checker type construction, inference,
overload resolution, contextual typing, or control-flow narrowing.

## Measured result

The reproducible command and complete machine-readable record are in
[`docs/evidence/ts7-oxc-spike-2026-08-17.json`](../evidence/ts7-oxc-spike-2026-08-17.json).
On macOS arm64 with Go 1.26.6 and Rust 1.95.0, six corpus cases exported 25
Go-oracle facts, 651 types, 556 declarations, 866 symbols, 482 signatures, and
6,368 graph edges. Repeated observations were identical for every case.

OXC parsed five of six selected source files. All 22 facts in those files
mapped and attached: 15 exact and seven normalized, with zero unmapped,
multiply-mapped, or actual-root transport mismatches. The mapped sample covered
21 `KindIdentifier` facts and one `KindStringLiteral` fact. The intentional
syntax-recovery file did not parse in OXC, leaving three Go facts explicitly
consumer-failed rather than silently discarded.

The graph contained 3,838 repeated edge targets out of 6,368 edges, a 602,701
parts-per-million sharing ratio. Producer output included 1,361 truncated and
seven error-state entities, no unsupported-state entities, and five cases with
producer budget truncation. Bounded inspection visited 2,883 nodes and 5,950
edges, reported 15 depth cutoffs, and truncated one fact at the independent
consumer budget. The two-pass snapshots occupied 1,315,514 bytes. The debug
consumer executable was 13,425,232 bytes and peak consumer RSS was 11,829,248
bytes.

The aggregate first one-shot pass was 634,312,462 ns and the immediately
repeated pass was 247,002,000 ns. These include a new `tsfacts` process for each
case and consumer decode/parse/inspection. They are local spike evidence, not a
general benchmark and not evidence for warm in-process project reuse.

## Decision

The first Go-owned category safe to mechanically port is schema-v1 occurrence
identity and attachment plumbing: request selection offsets, portable
occurrence tuples, OXC correlation, response-global fact indexing, and
TypeFacts side-table attachment. It may be ported only behind differential
comparison to the Go producer.

No Go semantic category is approved for replacement by this spike. Primitive
and literal type-record construction is the first proposed independent Rust
semantic candidate because its protocol surface is smallest, but it remains
Go-authoritative until an independent Rust producer exists and passes the
replacement gates below. Decoding a Go-produced primitive or correlating its
syntax is not semantic equivalence.

The producer protocol is unchanged. The evidence report is an internal schema,
and project-wide attachment preserves response-global fact indices entirely
inside the Rust consumer.

## Compatibility gates

Mechanical occurrence/attachment porting requires, on the complete corpus:

1. 1,000,000 parts-per-million mapping coverage for OXC-parseable sources;
2. zero multiply-mapped facts and zero actual-root transport mismatches;
3. byte-stable non-timing observations across the first and repeated pass;
4. explicit diagnostics for producer/exporter, protocol, mapping, and consumer
   failures; and
5. Go fallback for sources OXC cannot parse without losing recovery facts.

Replacing the first semantic category additionally requires an independent
Rust producer compared against the normalized Go oracle, with 100% exact
structured agreement for complete in-category facts, zero completeness or
state downgrades, zero new unsupported forms, deterministic response-local
identity and ordering, and differential CI across the corpus. New or changed
checker behavior remains Go-authoritative until it meets the same gate.

## Consequences

- The Rust seam can expand around transport and attachment without creating a
  second semantic authority.
- Intentional invalid syntax remains an explicit compatibility gap; OXC parser
  recovery cannot be inferred from valid-file coverage.
- Unmapped, ambiguous, unsupported, error, producer-truncated, and
  consumer-budget-truncated outcomes remain separate machine-readable states.
- Project loading, resolution, binding, symbol identity, type construction,
  inference, contextual typing, widening, overload/generic instantiation,
  control-flow narrowing, and recovery remain Go-authoritative.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0014](0014-isolate-the-oxc-reference-consumer.md)
- [ADR-0015](0015-attach-semantic-facts-without-expanding-graph-identity.md)
- [Issue #20](https://github.com/swernerx/typescript-semantic-kernel/issues/20)

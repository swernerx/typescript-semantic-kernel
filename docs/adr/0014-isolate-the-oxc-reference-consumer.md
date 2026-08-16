# ADR-0014: Isolate the OXC reference consumer

- Status: accepted
- Date: 2026-08-17
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

ADR-0013 defines a portable contract for correlating schema-v1 semantic facts
with consumer syntax nodes. Exercising that boundary against live OXC nodes
requires Rust-owned parser state and arena-local `NodeId`s. Putting those types
in the Go producer or its JSON Lines protocol would couple TypeScript 7's
semantic authority to one downstream syntax tree and make migration evidence
indistinguishable from a second implementation.

## Decision

Add an isolated Cargo workspace under `internal/oxc_reference`. It parses the
shared occurrence-map sources with OXC, projects relevant OXC nodes into the
portable ADR-0013 contract, and retains successful attachments as a typed
consumer-local `fact index -> NodeId` side table.

The workspace may serialize only the portable contract: response-local numeric
mapping IDs, normalized file names, UTF-8 spans, semantic syntax-kind names,
normalization rules, diagnostics, and coverage counters. Rust and OXC types do
not enter the TypeScript 7 producer protocol or Go packages.

The harness is migration evidence, not semantic authority. TypeScript 7's Go
checker remains the oracle. Future work may mechanically port linked Go
categories where useful, compare results against the Go oracle, and replace
categories one at a time only after an explicit compatibility threshold is
met.

## Consequences

- OXC upgrades and syntax projection stay isolated from the producer build.
- Shared JSON fixtures validate the same portable behavior in Go and Rust.
- Machine-readable reports expose exact and normalized mappings, unmapped and
  ambiguous facts, and coverage by syntax kind.
- A successful syntax correlation says nothing by itself about semantic
  equivalence; category replacement requires separate oracle evidence.

## Validation and review triggers

- CI runs the Rust workspace tests with the checked-in Cargo lockfile.
- Every shared v1 fixture must match its complete expected portable report.
- Representative sources must be parsed and traversed by OXC, and every emitted
  mapping must resolve to the typed `NodeId` retained by the consumer.
- New normalization shapes require a contract fixture before adapter support.
- Revisit this boundary only if a production consumer needs information that
  cannot be represented without changing the portable contract version.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0013](0013-correlate-occurrences-through-consumer-node-anchors.md)
- [Occurrence correlation contract](../occurrence-correlation.md)
- [Issue #35](https://github.com/swernerx/typescript-semantic-kernel/issues/35)

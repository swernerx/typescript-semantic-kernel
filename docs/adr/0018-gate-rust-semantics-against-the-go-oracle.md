# ADR-0018: Gate Rust semantics against the Go oracle

- Status: accepted
- Date: 2026-08-17
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

ADR-0017 introduced a versioned Rust-owned primitive/literal candidate over the
immutable schema-v1 graph emitted by TypeScript 7's Go checker. Its structured
records make a semantic comparison possible, but tests over one canonical
fixture do not establish corpus compatibility. Display strings also cannot
prove equivalence: a gate must compare fact identity, graph roots, structured
payloads, states, diagnostics, and truncation without collapsing different
failure modes.

The first candidate still consumes Go-produced facts. It is useful shadow
evidence, not an independent semantic producer and not authority to change the
TS7 producer protocol or replace the Go checker.

## Decision

`./internal/oxc_reference/run-conformance.sh` builds the real Go `tsfacts`
oracle, runs every case in `internal/semanticfacts/testdata/corpus/v0`, projects
every returned fact through the Rust primitive/literal candidate, and emits a
deterministic JSON report. `--output <path>` writes the same report for CI or
local inspection.

The comparator checks:

- the portable file/span/syntax fact identity and complete/recovered/truncated
  status;
- all ordered actual, contextual, widened, apparent, and declared view states
  and their effective response-local TypeIDs;
- each candidate TypeID, source kind/state/issues, structured primitive,
  literal, null-like, or union payload, union member identity, candidate state,
  and reason;
- the Go diagnostic count and producer budget report as observed by Rust; and
- OXC mapping coverage without dropping facts from sources that OXC cannot
  parse.

Every report entry is classified as `semantic`, `transport`, `mapping`,
`unsupported`, or `budget`. Ordering uses case name, category, fact index,
field path, code, and stable serialized values; the report contains no timing,
host path, or random identifier.

The shadow compatibility threshold is:

1. at least the seven complete, in-category primitive/literal records in the
   initial corpus baseline, preventing a vacuous pass if coverage disappears;
2. 1,000,000 parts-per-million exact agreement for those supported records;
3. zero unexplained semantic differences;
4. zero unexplained transport differences; and
5. explicit, non-blocking reporting of mapping differences and expected
   unsupported/error and producer/consumer budget states.

The CI command exits nonzero when either blocking count is nonzero or supported
compatibility falls below the threshold. Mapping does not block this shadow
semantic gate because the candidate can compare every decoded Go fact directly
and the corpus intentionally retains one syntax-recovery file that OXC cannot
parse. Mapping gaps remain visible and must be eliminated or bypassed by an
independent producer before replacement.

The initial corpus baseline is 25 facts across six cases, 125 compared roots
with 125 identity matches, and seven complete in-category records with seven
exact matches. It also reports the three facts in the known recovery-file
mapping gap and separately enumerates unsupported/error and budget/truncation
states. These counts describe the current shadow candidate; the executable
threshold, rather than these counts, is normative as the corpus expands.

## Replacement checklist

Passing the shadow gate does not approve replacement. A later ADR may transfer
one semantic category only after all of the following are true:

- Rust produces that category independently from source/project inputs instead
  of projecting the Go-produced graph.
- Go and Rust run the same pinned corpus, compiler revision, request,
  capabilities, and budgets.
- Every complete in-category fact and all five available roots reach 1,000,000
  ppm structured agreement with zero semantic or transport differences.
- Response-local graph identity, repeated roots, union member ordering,
  diagnostics, completeness, recovery, error, unsupported, and truncation
  behavior agree exactly.
- There are zero new unsupported forms, completeness downgrades, or unexplained
  mapping gaps for the category; expected unsupported and budget cases remain
  named fixtures rather than denominator exclusions hidden from the report.
- Repeated output is byte-stable and the gate runs as required CI on the
  supported matrix.
- Go remains an available fallback through rollout, and no TS7 producer
  protocol change is bundled with the authority switch.
- A category-specific ADR records fresh differential evidence and explicitly
  authorizes the switch.

## Consequences

- Semantic and transport regressions now fail CI with field-level JSON
  differences instead of display-text comparisons.
- Unsupported, error, recovery, mapping, and budget evidence stays observable
  without incorrectly failing the current shadow category.
- Corpus growth can increase the supported denominator without weakening the
  exact compatibility threshold.
- TypeScript 7's Go checker remains the semantic oracle and sole producer
  authority. This ADR makes no protocol or production-routing change.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0016](0016-port-occurrence-attachment-before-semantic-categories.md)
- [ADR-0017](0017-project-primitive-literal-candidates-from-go-graph-identity.md)
- [Issue #41](https://github.com/swernerx/typescript-semantic-kernel/issues/41)

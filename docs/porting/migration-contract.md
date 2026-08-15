# TypeScript semantic kernel migration contract

## Port decision

- **Problem:** Syntax-oriented tools need TypeScript-compatible semantic facts
  without linking the compiler's internal Go object graph.
- **Delivery shape:** Incremental vertical slices. The authoritative
  `microsoft/typescript-go` parser, project loader, binder, resolver, and checker
  remain intact behind a narrow adapter until reachability and compatibility
  evidence justifies pruning.
- **Authority:** This repository may implement, test, and merge the adapter and
  its conformance corpus. Replacing checker categories with another backend
  requires the compatibility threshold described by a later accepted RFC or
  ADR.

## Equivalence boundary

The kernel preserves TypeScript 7 project loading, resolution, binding,
contextual typing, inference, and control-flow semantics for every fact it
claims as complete. The initial public boundary is a versioned process protocol
rather than Go `Type`, `Symbol`, or AST pointers.

Schema v1 currently guarantees:

- normalized project-relative file identities;
- zero-based, half-open UTF-8 byte spans;
- deterministic record ordering for an identical request and source tree;
- explicit complete, recovered, and truncated state;
- stable per-response type IDs that never reuse compiler-internal numeric IDs.
- an explicit actual type plus contextual, widened, apparent, and declared
  roots whose absence is explained by machine-readable view states;
- mutually exclusive annotation and inferred origin views for symbol-backed
  value occurrences, plus explicit control-flow-narrowed views relative to
  unflowed checker types for the selected symbol and property receiver;
- response-local symbol and declaration handles with deterministic declaration
  ordering;
- explicit alias-to-target edges without collapsing the local alias;
- logical TypeScript default-library file identities that do not expose host
  installation paths.

The initial slice does not guarantee complete structural serialization of every
TypeScript type, inference traces, declarations outside the project root or
TypeScript default libraries, project references, or an OXC bridge. Those are
migration queue items, not implicit passes.

## Equivalence oracle

The Go checker is the first authoritative backend. Focused structured fixtures
exercise the adapter directly. The corpus will grow to differential fixtures
against TypeScript's observable type and symbol behavior and will later be
shared by alternate backends.

## Gate ladder

1. Focused `internal/tsfacts` tests pass, including non-ASCII spans,
   deterministic output, recovery, annotation/inference provenance, narrowing
   baselines, contextual typing, bounded type serialization, aliases, merged
   declarations, and declaration file identity.
2. `go test ./internal/tsfacts ./cmd/tsfacts` passes.
3. The new command builds through the repository-native build graph.
4. The complete repository test suite passes without reduced test counts.
5. Format and lint checks pass.
6. CI passes on the upstream-supported matrix.
7. Phase 0 meets every acceptance criterion in RFC 0001 before that RFC becomes
   Accepted.

## Upstream synchronization

The `upstream` remote remains `microsoft/typescript-go`. Each synchronization
records the upstream commit, rebases or merges the small adapter patch series,
runs the gate ladder, and updates the protocol header revision only after the
new baseline is verified. Upstream code is not reformatted or reorganized as
part of adapter work.

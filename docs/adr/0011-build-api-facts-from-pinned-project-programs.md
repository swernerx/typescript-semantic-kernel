# ADR-0011: Build API facts from pinned project programs

- Status: accepted
- Date: 2026-08-16
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

The asynchronous TypeScript 7 API already gives clients explicit `Snapshot`
and `Project` objects. Re-running the one-shot `tsfacts` project loader inside a
new API method would ignore that lifecycle: unsaved overlays and temporary
snapshots could disagree with the returned facts, and every request would pay
for a second parse, bind, and project load.

The semantic facts model and graph builder already live independently of their
JSON Lines transport, but their only entry point constructed a compiler program
from a config file.

## Decision

`semanticfacts.AnalyzeProgram` accepts an existing compiler program and runs the
same validation, selection, diagnostics, graph interning, normalization, and
budget logic as the one-shot command. `semanticfacts.Analyze` remains the
configured-project convenience entry point and delegates to that shared path.

The asynchronous API exposes `Project.getSemanticSnapshot(request, signal)`.
The wire request carries the owning snapshot and project handles plus the
schema-v1 scope. The Go API session resolves those handles and delegates to
`AnalyzeProgram`; it does not serialize through JSON Lines or load another
project.

The returned v0 envelope contains the same ordered `header`, `files`, `types`,
`declarations`, `symbols`, `signatures`, and `facts` tables as the process
contract. An optional `AbortSignal` becomes a JSON-RPC cancellation token, and
the server cancels the request context on `$/cancelRequest`.

## Considered options

### Invoke `tsfacts` as a subprocess

This would preserve the executable boundary but discard warm project state,
duplicate serialization, and make overlays invisible.

### Rebuild a compiler program inside the API handler

This avoids a subprocess but still violates snapshot identity and duplicates
project work.

### Add a second API-specific facts model

This would let the TypeScript API evolve independently, but it creates a second
normalization contract and complicates the Rust decoder and conformance oracle.

## Consequences

- Semantic facts match the exact snapshot the caller holds, including temporary
  file text.
- The API and JSON Lines boundaries share one graph implementation and one set
  of compatibility rules.
- The API surface remains experimental and schema-versioned; future envelopes
  can be negotiated without exposing Go checker objects.
- General JSON-RPC cancellation is now tracked per request by the asynchronous
  connection.

## Validation and review triggers

- API tests compare repeated envelopes byte for byte, distinguish base and
  temporary snapshots, and cover invalid files and cancellation.
- Native-preview tests exercise the public TypeScript method and `AbortSignal`.
- Revisit the process-versus-session boundary after the Phase 3 measurements in
  RFC 0001.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0007](0007-separate-semantic-snapshot-building-from-json-lines.md)
- [Semantic facts protocol](../tsfacts-protocol.md)
- [Issue #13](https://github.com/swernerx/typescript-semantic-kernel/issues/13)

# Normalized corpus snapshots v0

These JSON Lines goldens cover the highest-value semantic-facts corpus cases:
advanced type records, occurrence-specific views, and the combined vertical
slice. The conformance test replaces TypeScript version metadata and sorts
response-local record tables before comparison. It also replaces the unstable
numeric suffix on TypeScript's internal well-known-symbol names. Semantic IDs
and edge order remain unchanged because tuple positions, overload order, and
union members are part of the evidence.

Regenerate the snapshots intentionally with:

```sh
go test ./internal/semanticfacts \
  -run TestSemanticFactsCorpusSnapshotsAreNormalizedAndStable \
  -update-semantic-facts-goldens
```

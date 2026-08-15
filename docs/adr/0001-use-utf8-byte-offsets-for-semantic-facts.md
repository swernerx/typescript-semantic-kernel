# ADR-0001: Use UTF-8 byte offsets for semantic facts schema v1

- Status: accepted
- Date: 2026-08-15
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

RFC 0001 requires every semantic occurrence to carry an explicit offset
encoding. TypeScript traditionally exposes UTF-16 code-unit positions, while
the Go compiler and OXC both operate naturally on UTF-8 source bytes. The
protocol must not leave the unit implicit because offsets diverge as soon as a
file contains non-ASCII text.

## Decision

Semantic facts schema version 1 uses zero-based, half-open UTF-8 byte offsets.
Every protocol header declares the encoding as `utf8-bytes`. Requests and
responses use the same unit, and conformance fixtures include non-ASCII source
text to prevent accidental conversion to rune or UTF-16 positions.

## Considered options

### UTF-8 byte offsets

This matches Go string indices, parser spans, OXC spans, and direct slicing of
source buffers. It avoids a conversion at the intended Rust consumer boundary.

### UTF-16 code-unit offsets

This matches the established TypeScript and language-server convention, but it
would require conversion at both the Go checker boundary and the initial OXC
consumer. A later transport may add UTF-16 as a negotiated encoding if an
editor-oriented consumer needs it.

## Consequences

- OXC can correlate facts with its native spans without offset conversion.
- JavaScript clients must not treat offsets as string indices without first
  converting from UTF-8 bytes.
- All fixtures involving offsets must include the declared encoding.
- Adding another encoding requires a schema-compatible negotiation rule or a
  new schema version; silently changing the unit is forbidden.

## Validation and review triggers

- A non-ASCII fixture verifies exact byte spans.
- Revisit the decision if an editor protocol becomes the primary transport or
  measurements show offset conversion dominates a supported workflow.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)

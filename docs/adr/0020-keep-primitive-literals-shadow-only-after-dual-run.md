# ADR-0020: Keep primitive/literal production shadow-only after the controlled dual-run

- Status: accepted
- Date: 2026-08-17
- Deciders: TypeScript Semantic Kernel maintainers
- Supersedes: —
- Superseded by: —

## Context

ADR-0019 established an independent Rust/OXC primitive/literal producer and an
exact differential gate against the TypeScript 7 Go checker. The expanded
corpus proves a useful semantic slice, but passing that gate alone does not show
that the category can be served, rolled back, or compared under a
production-equivalent runtime and memory boundary.

Issue #47 requires a controlled dual-run over the same checked-out corpus,
compiler revision, request, capabilities, and budgets. The evidence must retain
semantic, transport, mapping, unsupported, and budget classifications while
also recording runtime, memory, and output size without turning noisy host
measurements into deterministic compatibility fields.

## Decision

`./internal/oxc_reference/run-rollout.sh --output <path>` builds the real Go
`tsfacts` command and the release Rust reference binary, then executes two
complete ordered dual-runs. Each run sends the same manifest-derived requests
to the pinned TypeScript compiler and independent Rust producer. Only the
comparator receives both outputs.

The embedded conformance report is deterministic and must be byte-identical
between the two runs. It records the repository and TypeScript revisions,
request schema, project, ordered capabilities, budgets, selections, all five
roots, response-local identity, structured payloads, diagnostics,
completeness, recovery, truncation, unsupported states, mapping, and classified
differences. Unexplained semantic, transport, or mapping differences fail the
command.

Issue #52 resolves the seven named rollout observations without broadening the
producer category. Conformance schema v5 keeps every selected fact in a second,
CI-enforced accounting denominator, which must remain exactly 100%. The
supported-record compatibility metric therefore cannot hide unsupported,
budget, or mapping selections. Each of the four unsupported selections and
three recovery-file mapping gaps is a stable regression fixture with a
machine-readable owner and concrete action required for reclassification. A
changed code, state, diagnostic, or mapping outcome is unexplained and blocks
the gate.

Runtime and output sizes are retained as two ordered measurement samples.
Artifact sizes and the controller's peak or current resident memory are
recorded with their measurement method and scope. These measurements are
characterization data: the Go lane includes a one-shot process and JSON Lines
transport, while Rust runs in-process, and controller RSS excludes the Go child
process. The measured fields are therefore not part of the byte-stability gate
and do not establish a performance advantage.

The primitive/literal category is **not ready for a later authority decision**.
Rust remains shadow-only. Go remains both the serving semantic authority and
the production fallback. There is no authority switch, TS7 producer protocol
change, external consumer behavior change, or Palamedes change.

## Resolved rollout observations

- The local enum literal remains owned by `rust-primitive-literal-producer`
  until local enum declaration and member-value resolution is implemented.
- The value and type import selections remain owned by
  `rust-project-resolution` until project-aware cross-file resolution exists.
- The object selection remains owned by `object-category-rollout`; object
  semantics are intentionally not folded into the primitive/literal producer.
- The three recovery selections remain owned by `oxc-occurrence-mapping` until
  OXC parses or recovers the file and supplies each exact `NodeId` mapping.

These are resolved, explicit limitations rather than authority-readiness
blockers or hidden exclusions. Their fixture contracts remain CI-blocking when
the observed behavior changes.

## Remaining authority blockers

- Rust is not integrated into the serving path, so production fallback,
  rollback, and shadow observation at that boundary have not been exercised.
- Runtime and output-size samples do not yet use one production-equivalent
  process or library boundary for both implementations.
- The evidence does not isolate peak resident memory for the Go child process,
  so per-producer memory parity is not established.

A later authority proposal must close or explicitly re-scope these blockers,
rerun the supported platform matrix, preserve Go fallback, and create a new ADR
that authorizes the category-specific switch. This record does not authorize
that proposal.

## Consequences

- CI now retains one structured rollout artifact containing deterministic
  conformance, coverage, compatibility, mapping, runtime, memory, and output
  size evidence.
- Host-dependent measurements remain reproducible through a documented command
  without weakening the exact semantic gate.
- A green rollout job means that shadow comparison is healthy. It does not mean
  the category is production-ready or approved to replace Go.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0019](0019-compute-primitive-literals-independently-in-rust.md)
- [Checked rollout evidence](../evidence/primitive-literal-rollout-2026-08-17.json)
- [Issue #47](https://github.com/swernerx/typescript-semantic-kernel/issues/47)
- [Issue #52](https://github.com/swernerx/typescript-semantic-kernel/issues/52)

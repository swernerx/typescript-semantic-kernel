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

Issue #53 selects `one-shot-child-process-shadow` as the production-equivalent
observation boundary. For every classified corpus case, the release rollout
controller starts the real `tsfacts` command as the Go serving child and the
release `oxc-occurrence-map primitive-shadow-worker` command as the Rust shadow
child. Both receive the same ordered project selections and equivalent limits.
Only the Go child's stdout is eligible to become the served response; Rust
stdout is observation-only and uses an internal request/response shape that is
not the TS7 producer protocol.

The serving controller's failure state machine is executable library code. A
Rust failure is observed, preserves the Go response, disables subsequent
shadow execution, and can be re-enabled only by an explicit reset. A Go
failure is returned and never masked by Rust. Unit tests and every rollout
report exercise failure, rollback, skipped-after-rollback, reset, and Go
failure paths.

Runtime, raw producer-output size, artifact size, and peak resident memory are
retained as two ordered measurement samples. Both runtime lanes now cover the
same one-shot child-process boundary. On Unix, the controller uses `wait4`
`ru_maxrss` for each child and aggregates the maximum case-process RSS per
producer. Raw output sizes deliberately cover different internal schemas and
the sequential samples remain characterization data, not performance claims or
compatibility thresholds.

With these production-integration and measurement gaps closed, the checked
evidence is **ready to inform a later authority decision**. This ADR still
keeps Rust shadow-only. Go remains both the serving semantic authority and the
production fallback. There is no authority switch, TS7 producer protocol
change, external consumer behavior change, or Palamedes change. A switch still
requires an explicit later proposal and ADR.

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

## Remaining caveats

- The four unsupported and three recovery-file observations remain explicit,
  owned limitations; evidence readiness does not make them supported.
- Raw Go and Rust output-byte counts cover different internal payload schemas
  and must not be interpreted as wire-protocol parity.
- The sequential one-shot samples are not daemon throughput or latency
  benchmarks.
- Peak RSS uses Unix `wait4`; a non-Unix release must supply or justify a
  comparable child-process method.

A later authority proposal must assess these caveats, rerun the supported
platform matrix, preserve Go fallback, and create a new ADR that authorizes any
category-specific switch. This record does not authorize that proposal.

## Consequences

- CI now retains one structured rollout artifact containing deterministic
  conformance, coverage, compatibility, mapping, runtime, memory, and output
  size evidence.
- Host-dependent measurements remain reproducible through a documented command
  without weakening the exact semantic gate.
- A green rollout job means that exact comparison, production-equivalent shadow
  observation, rollback, and scoped measurements are healthy. It does not
  approve replacing Go.

## References

- [RFC 0001](../../rfcs/0001-semantic-facts-kernel.md)
- [ADR-0019](0019-compute-primitive-literals-independently-in-rust.md)
- [Checked rollout evidence](../evidence/primitive-literal-rollout-2026-08-17.json)
- [Issue #47](https://github.com/swernerx/typescript-semantic-kernel/issues/47)
- [Issue #52](https://github.com/swernerx/typescript-semantic-kernel/issues/52)
- [Issue #53](https://github.com/swernerx/typescript-semantic-kernel/issues/53)

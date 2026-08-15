# `tsfacts` protocol spike

`tsfacts` is the first process boundary for RFC 0001. It reads one schema-v1
request from standard input and writes JSON Lines to standard output.

```sh
go run ./cmd/tsfacts <<'JSON'
{"schemaVersion":1,"project":"tsconfig.json","files":["src/example.ts"],"selections":[{"file":"src/example.ts","start":120,"end":125}]}
JSON
```

Offsets are zero-based, half-open UTF-8 byte offsets. A selection must fit
inside one source token in the current spike. The response contains, in order:

1. one `header` record;
2. one `file` record for each selected file, sorted by project-relative ID;
3. interned `type` records in deterministic discovery order;
4. one `fact` record per requested selection.

The first slice exposes the checker's `getTypeAtLocation` result as
`typeAtLocation`. This view reflects control-flow narrowing where TypeScript
applies it without mislabelling every ordinary occurrence as narrowed. When
distinct and available the command also emits contextual, widened, and
constraint views. Primitive, literal, union, intersection, type parameter,
object, and callable categories have explicit wire kinds. Object details and
type categories not yet represented structurally are marked truncated and
retain display text only as diagnostic metadata.

Each fact reports three independent states:

- `complete`: every referenced type view is structurally represented and the
  selected file has no diagnostics;
- `recovered`: the checker produced the fact while the file had syntactic or
  semantic diagnostics;
- `truncated`: at least one referenced type view exceeded the current schema or
  serializer limits.

Per-response IDs such as `type:1` are deterministic handles only. They are not
compiler-internal IDs and must not be persisted across requests.

This spike intentionally omits symbol/declaration records, annotation and
inference views, configurable limits, diagnostic payloads, project references,
daemon reuse, and the OXC bridge. These remain explicit Phase 0 work rather
than undocumented protocol behavior.

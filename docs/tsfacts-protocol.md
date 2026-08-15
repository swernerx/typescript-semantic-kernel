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
2. `file` records for selected files and referenced declaration files, sorted
   by logical ID;
3. interned `type` records in deterministic discovery order;
4. interned `declaration` records;
5. interned `symbol` records, including alias-target edges;
6. one `fact` record per requested selection.

The first slice exposes the checker's `getTypeAtLocation` result as
`typeAtLocation`. This view reflects control-flow narrowing where TypeScript
applies it without mislabelling every ordinary occurrence as narrowed. When
distinct and available the command also emits contextual, widened, and
constraint views. Primitive, literal, union, intersection, type parameter,
object, and callable categories have explicit wire kinds. Object details and
type categories not yet represented structurally are marked truncated and
retain display text only as diagnostic metadata.

When TypeScript exposes a symbol at the selected token, the fact contains a
response-local `symbol` handle and its direct declaration handles. Symbol
records contain a display name, stable protocol roles, all representable source
declarations, and an `aliasedSymbol` edge for imports or exports. Aliases are
not collapsed: a consumer can inspect both the local binding and the resolved
target. Merged declarations are sorted by file and span before IDs are
allocated.

Schema-v1 symbol roles are `alias`, `variable`, `property`, `enum_member`,
`function`, `class`, `interface`, `enum`, `module`, `method`, `constructor`,
`accessor`, `signature`, `type_parameter`, `type_alias`, `optional`, and
`transient`. A symbol can have several roles. `unknown` is emitted only when no
schema-v1 role represents the checker symbol.

Declaration spans identify the declaration name when one exists and otherwise
the declaration node. Project files use normalized project-relative IDs.
TypeScript default-library declarations use logical IDs such as
`typescript/lib/lib.es5.d.ts`, so output never depends on the installation
directory. A declaration outside both supported namespaces is omitted and the
owning symbol is marked truncated; absolute host paths are never emitted.

A `file` record has `origin` set to `project` or `typescript-lib`. Selected
files additionally carry `selected: true` and `diagnosticCount`. A file emitted
only to identify a declaration omits both fields because it was not selected
for diagnostic collection.

Each fact reports three independent states:

- `complete`: every referenced type view and symbol edge is represented and the
  selected file has no diagnostics;
- `recovered`: the checker produced the fact while the file had syntactic or
  semantic diagnostics;
- `truncated`: at least one referenced type view exceeded the current schema or
  serializer limits.

Per-response IDs such as `type:1`, `symbol:1`, and `declaration:1` are
deterministic handles only. They are not compiler-internal IDs and must not be
persisted across requests. See [ADR-0002](adr/0002-use-response-local-symbol-and-declaration-ids.md).

This spike intentionally omits annotation and inference views, configurable
limits, diagnostic payloads, project references, daemon reuse, and the OXC
bridge. These remain explicit Phase 0 work rather than undocumented protocol
behavior.

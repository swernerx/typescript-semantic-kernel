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
6. interned `signature` records;
7. one `fact` record per requested selection.

The first slice exposes the checker's `getTypeAtLocation` result as the required
`actualType` view. It is the type TypeScript observes at the selected source
occurrence. The equal `typeAtLocation` field remains available for schema-v1
compatibility.

Each occurrence is identified by the tuple of its normalized file ID, zero-based
half-open UTF-8 token span, and TypeScript syntax kind. Flow-sensitive uses are
separate occurrences even when they resolve to the same symbol. Repeating an
identical request selection repeats the same occurrence fact in request order;
it does not create a new semantic identity.

For a value occurrence backed by a symbol, schema v1 additionally classifies
the origin and flow state:

- `annotationType` is the type written on an unambiguous variable, parameter,
  property declaration, or property signature. It represents the source type
  node, so an optional `value?: string` has annotation type `string` even though
  its declared checker type includes `undefined`.
- `inferredType` is the unflowed symbol type when no supported direct annotation
  supplies the symbol's whole type. `annotationType` and `inferredType` are
  mutually exclusive.
- `narrowedType` is present only when `typeAtLocation` differs from the unflowed
  symbol type or a property is reached through a flow-narrowed receiver. Its ID
  is therefore also the `typeAtLocation` ID; the additional field names the
  control-flow provenance of that observation.

Containing annotations are not promoted to the selected symbol. In particular,
an annotation on a destructuring pattern describes the pattern input, and a
function return annotation describes the return type rather than the callable
symbol. Type-only occurrences do not receive value-origin or narrowing views.
See [ADR-0003](adr/0003-classify-symbol-backed-type-view-provenance.md).

When distinct and available the command also emits contextual, widened,
apparent, declared, and constraint views. The declared view is the unflowed
value-symbol type at value occurrences or the checker's declared type at type
occurrences. The apparent view applies the checker's apparent-type operation to
the actual root. Those views are independent of the annotation/inference
classification.

Required `typeViewStates` entries explain each contract view. `available` means
the root is present, `same-as-actual` means it was omitted only to avoid a
duplicate edge, `inapplicable` means the operation has no meaning for that
syntax, and `unavailable` means the operation applies but the checker returned
no type. Truncation is separate: an available root may point to an explicitly
incomplete type node. A failure to obtain `actualType` fails the request instead
of emitting an ambiguous fact. See [ADR-0004](adr/0004-make-optional-type-view-availability-explicit.md).

Primitive, literal, union, intersection, type parameter, object, and callable
categories have explicit wire kinds.
Object details and type categories not yet represented structurally are marked
truncated and retain display text only as diagnostic metadata.

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

The graph contract additionally reserves response-local `signature:` handles
and explicit cross-table edges. Types can reference members, symbols, generic
targets and arguments, constraints, defaults, properties, and call, construct,
or index signatures. Symbols can reference declarations, aliases, types, and
members. Signatures reference their declaration, type parameters, `this` type,
parameter symbols, and return type. The exporter allocates an ID before walking
edges; forward references, sharing, self-cycles, and mutually recursive cycles
are therefore valid and record order never implies ownership.

Every response is checked for unique namespaced IDs, resolvable edges, coherent
completeness, and known type and signature variants before output begins. An
unknown variant fails explicitly instead of being guessed. The existing
`opaque` and `truncated` kinds are named schema variants, not unknown fallbacks.
See [ADR-0005](adr/0005-use-response-local-referential-graph-tables.md).

This spike intentionally omits inference traces, configurable limits,
diagnostic payloads, project references, daemon reuse, and the OXC bridge. These
remain explicit Phase 0 work rather than undocumented protocol behavior.

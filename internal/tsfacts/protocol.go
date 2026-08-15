package tsfacts

const (
	SchemaVersion    = 1
	OffsetEncoding   = "utf8-bytes"
	UpstreamRevision = "1bcfa18d79a3be41772223d5c05dfe4480e614ff"
)

type Request struct {
	SchemaVersion int         `json:"schemaVersion"`
	Project       string      `json:"project"`
	Files         []string    `json:"files,omitzero"`
	Selections    []Selection `json:"selections"`
}

type Selection struct {
	File  string `json:"file"`
	Start int    `json:"start"`
	End   int    `json:"end"`
}

type Span struct {
	Start int `json:"start"`
	End   int `json:"end"`
}

type TypeID string

type HeaderRecord struct {
	Record             string `json:"record"`
	SchemaVersion      int    `json:"schemaVersion"`
	TypeScriptVersion  string `json:"typescriptVersion"`
	TypeScriptRevision string `json:"typescriptRevision"`
	OffsetEncoding     string `json:"offsetEncoding"`
	Project            string `json:"project"`
	CompilerOptions    any    `json:"compilerOptions"`
	DiagnosticCount    int    `json:"diagnosticCount"`
}

type FileRecord struct {
	Record          string `json:"record"`
	ID              string `json:"id"`
	DiagnosticCount int    `json:"diagnosticCount"`
}

type LiteralValue struct {
	Kind  string `json:"kind"`
	Value string `json:"value"`
}

type TypeRecord struct {
	Record     string        `json:"record"`
	ID         TypeID        `json:"id"`
	TypeKind   string        `json:"typeKind"`
	Display    string        `json:"display"`
	Flags      []string      `json:"flags"`
	Members    []TypeID      `json:"members,omitzero"`
	Constraint TypeID        `json:"constraint,omitzero"`
	Literal    *LiteralValue `json:"literal,omitzero"`
	Complete   bool          `json:"complete"`
	Truncated  bool          `json:"truncated"`
}

type FactRecord struct {
	Record         string `json:"record"`
	File           string `json:"file"`
	Span           Span   `json:"span"`
	SyntaxKind     string `json:"syntaxKind"`
	TypeAtLocation TypeID `json:"typeAtLocation"`
	ContextualType TypeID `json:"contextualType,omitzero"`
	WidenedType    TypeID `json:"widenedType,omitzero"`
	ConstraintType TypeID `json:"constraintType,omitzero"`
	Complete       bool   `json:"complete"`
	Recovered      bool   `json:"recovered"`
	Truncated      bool   `json:"truncated"`
}

type Result struct {
	Header HeaderRecord
	Files  []FileRecord
	Types  []TypeRecord
	Facts  []FactRecord
}

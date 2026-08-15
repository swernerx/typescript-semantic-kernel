package tsfacts

const (
	SchemaVersion    = 1
	OffsetEncoding   = "utf8-bytes"
	UpstreamRevision = "1bcfa18d79a3be41772223d5c05dfe4480e614ff"

	TypeViewAvailable    = "available"
	TypeViewSameAsActual = "same-as-actual"
	TypeViewInapplicable = "inapplicable"
	TypeViewUnavailable  = "unavailable"
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

type (
	TypeID        string
	SymbolID      string
	DeclarationID string
)

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
	Origin          string `json:"origin"`
	Selected        bool   `json:"selected,omitzero"`
	DiagnosticCount *int   `json:"diagnosticCount,omitzero"`
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

type DeclarationRecord struct {
	Record     string        `json:"record"`
	ID         DeclarationID `json:"id"`
	File       string        `json:"file"`
	Span       Span          `json:"span"`
	SyntaxKind string        `json:"syntaxKind"`
}

type SymbolRecord struct {
	Record        string          `json:"record"`
	ID            SymbolID        `json:"id"`
	Name          string          `json:"name"`
	Roles         []string        `json:"roles"`
	Declarations  []DeclarationID `json:"declarations,omitzero"`
	AliasedSymbol SymbolID        `json:"aliasedSymbol,omitzero"`
	Complete      bool            `json:"complete"`
	Truncated     bool            `json:"truncated"`
}

type TypeViewStates struct {
	Actual     string `json:"actual"`
	Contextual string `json:"contextual"`
	Widened    string `json:"widened"`
	Apparent   string `json:"apparent"`
	Declared   string `json:"declared"`
}

type FactRecord struct {
	Record         string          `json:"record"`
	File           string          `json:"file"`
	Span           Span            `json:"span"`
	SyntaxKind     string          `json:"syntaxKind"`
	ActualType     TypeID          `json:"actualType"`
	TypeAtLocation TypeID          `json:"typeAtLocation"`
	AnnotationType TypeID          `json:"annotationType,omitzero"`
	InferredType   TypeID          `json:"inferredType,omitzero"`
	ContextualType TypeID          `json:"contextualType,omitzero"`
	WidenedType    TypeID          `json:"widenedType,omitzero"`
	ApparentType   TypeID          `json:"apparentType,omitzero"`
	DeclaredType   TypeID          `json:"declaredType,omitzero"`
	NarrowedType   TypeID          `json:"narrowedType,omitzero"`
	ConstraintType TypeID          `json:"constraintType,omitzero"`
	TypeViewStates TypeViewStates  `json:"typeViewStates"`
	Symbol         SymbolID        `json:"symbol,omitzero"`
	Declarations   []DeclarationID `json:"declarations,omitzero"`
	Complete       bool            `json:"complete"`
	Recovered      bool            `json:"recovered"`
	Truncated      bool            `json:"truncated"`
}

type Result struct {
	Header       HeaderRecord
	Files        []FileRecord
	Types        []TypeRecord
	Declarations []DeclarationRecord
	Symbols      []SymbolRecord
	Facts        []FactRecord
}

package semanticfacts

const (
	SchemaVersion    = 1
	OffsetEncoding   = "utf8-bytes"
	UpstreamRevision = "1bcfa18d79a3be41772223d5c05dfe4480e614ff"

	TypeViewAvailable    = "available"
	TypeViewSameAsActual = "same-as-actual"
	TypeViewInapplicable = "inapplicable"
	TypeViewUnavailable  = "unavailable"

	EntityStateComplete    = "complete"
	EntityStateTruncated   = "truncated"
	EntityStateUnsupported = "unsupported"
	EntityStateError       = "error"
)

type Request struct {
	SchemaVersion        int          `json:"schemaVersion"`
	RequiredCapabilities []string     `json:"requiredCapabilities,omitzero"`
	Budgets              BudgetLimits `json:"budgets,omitzero"`
	Project              string       `json:"project"`
	Files                []string     `json:"files,omitzero"`
	Selections           []Selection  `json:"selections"`
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
	SignatureID   string
)

type HeaderRecord struct {
	Record             string       `json:"record"`
	SchemaVersion      int          `json:"schemaVersion"`
	TypeScriptVersion  string       `json:"typescriptVersion"`
	TypeScriptRevision string       `json:"typescriptRevision"`
	OffsetEncoding     string       `json:"offsetEncoding"`
	Capabilities       []string     `json:"capabilities"`
	Budgets            BudgetReport `json:"budgets"`
	Project            string       `json:"project"`
	CompilerOptions    any          `json:"compilerOptions"`
	DiagnosticCount    int          `json:"diagnosticCount"`
}

type BudgetLimits struct {
	MaxTypeNodes int `json:"maxTypeNodes,omitzero"`
	MaxTypeDepth int `json:"maxTypeDepth,omitzero"`
}

type BudgetReport struct {
	Limits               BudgetLimits `json:"limits"`
	TypeNodesUsed        int          `json:"typeNodesUsed"`
	MaxTypeDepthObserved int          `json:"maxTypeDepthObserved"`
	Truncated            bool         `json:"truncated"`
}

type GraphIssue struct {
	Code  string `json:"code"`
	Limit int    `json:"limit,omitzero"`
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

type ArrayTypeDetails struct {
	Readonly bool `json:"readonly"`
}

type TupleElementDetails struct {
	Kind  string `json:"kind"`
	Label string `json:"label,omitzero"`
}

type TupleTypeDetails struct {
	Readonly bool                  `json:"readonly"`
	Elements []TupleElementDetails `json:"elements"`
}

type ConditionalTypeDetails struct {
	CheckType           TypeID   `json:"checkType"`
	ExtendsType         TypeID   `json:"extendsType"`
	TrueType            TypeID   `json:"trueType"`
	FalseType           TypeID   `json:"falseType"`
	InferTypeParameters []TypeID `json:"inferTypeParameters,omitzero"`
	Distributive        bool     `json:"distributive,omitzero"`
}

type MappedTypeDetails struct {
	TypeParameter    TypeID `json:"typeParameter"`
	ConstraintType   TypeID `json:"constraintType"`
	NameType         TypeID `json:"nameType,omitzero"`
	TemplateType     TypeID `json:"templateType"`
	ModifiersType    TypeID `json:"modifiersType,omitzero"`
	ReadonlyModifier string `json:"readonlyModifier"`
	OptionalModifier string `json:"optionalModifier"`
}

type IndexedAccessTypeDetails struct {
	ObjectType TypeID `json:"objectType"`
	IndexType  TypeID `json:"indexType"`
}

type TemplateLiteralTypeDetails struct {
	Texts []string `json:"texts"`
	Types []TypeID `json:"types"`
}

type SubstitutionTypeDetails struct {
	BaseType   TypeID `json:"baseType"`
	Constraint TypeID `json:"constraint"`
}

type TypeRecord struct {
	Record              string                      `json:"record"`
	ID                  TypeID                      `json:"id"`
	TypeKind            string                      `json:"typeKind"`
	Display             string                      `json:"display"`
	Flags               []string                    `json:"flags"`
	Members             []TypeID                    `json:"members,omitzero"`
	Symbol              SymbolID                    `json:"symbol,omitzero"`
	Target              TypeID                      `json:"target,omitzero"`
	TypeArguments       []TypeID                    `json:"typeArguments,omitzero"`
	Constraint          TypeID                      `json:"constraint,omitzero"`
	Default             TypeID                      `json:"default,omitzero"`
	Properties          []SymbolID                  `json:"properties,omitzero"`
	CallSignatures      []SignatureID               `json:"callSignatures,omitzero"`
	ConstructSignatures []SignatureID               `json:"constructSignatures,omitzero"`
	IndexSignatures     []SignatureID               `json:"indexSignatures,omitzero"`
	Literal             *LiteralValue               `json:"literal,omitzero"`
	Array               *ArrayTypeDetails           `json:"array,omitzero"`
	Tuple               *TupleTypeDetails           `json:"tuple,omitzero"`
	Conditional         *ConditionalTypeDetails     `json:"conditional,omitzero"`
	Mapped              *MappedTypeDetails          `json:"mapped,omitzero"`
	IndexedAccess       *IndexedAccessTypeDetails   `json:"indexedAccess,omitzero"`
	TemplateLiteral     *TemplateLiteralTypeDetails `json:"templateLiteral,omitzero"`
	Substitution        *SubstitutionTypeDetails    `json:"substitution,omitzero"`
	State               string                      `json:"state"`
	Issues              []GraphIssue                `json:"issues,omitzero"`
	Complete            bool                        `json:"complete"`
	Truncated           bool                        `json:"truncated"`
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
	Type          TypeID          `json:"type,omitzero"`
	DeclaredType  TypeID          `json:"declaredType,omitzero"`
	Members       []SymbolID      `json:"members,omitzero"`
	State         string          `json:"state"`
	Issues        []GraphIssue    `json:"issues,omitzero"`
	Complete      bool            `json:"complete"`
	Truncated     bool            `json:"truncated"`
}

type SignatureRecord struct {
	Record           string        `json:"record"`
	ID               SignatureID   `json:"id"`
	SignatureKind    string        `json:"signatureKind"`
	Declaration      DeclarationID `json:"declaration,omitzero"`
	Target           SignatureID   `json:"target,omitzero"`
	TypeArguments    []TypeID      `json:"typeArguments,omitzero"`
	TypeParameters   []TypeID      `json:"typeParameters,omitzero"`
	ThisType         TypeID        `json:"thisType,omitzero"`
	Parameters       []SymbolID    `json:"parameters,omitzero"`
	MinArgumentCount int           `json:"minArgumentCount,omitzero"`
	HasRestParameter bool          `json:"hasRestParameter,omitzero"`
	IndexKeyType     TypeID        `json:"indexKeyType,omitzero"`
	Readonly         bool          `json:"readonly,omitzero"`
	ReturnType       TypeID        `json:"returnType"`
	State            string        `json:"state"`
	Issues           []GraphIssue  `json:"issues,omitzero"`
	Complete         bool          `json:"complete"`
	Truncated        bool          `json:"truncated"`
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
	Header       HeaderRecord        `json:"header"`
	Files        []FileRecord        `json:"files"`
	Types        []TypeRecord        `json:"types"`
	Declarations []DeclarationRecord `json:"declarations"`
	Symbols      []SymbolRecord      `json:"symbols"`
	Signatures   []SignatureRecord   `json:"signatures"`
	Facts        []FactRecord        `json:"facts"`
}

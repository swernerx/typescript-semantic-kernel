package semanticfacts

import (
	"cmp"
	"fmt"
	"slices"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/checker"
)

type typeInterner struct {
	checker          *checker.Checker
	limits           BudgetLimits
	coreComposite    bool
	references       bool
	signatureGraph   bool
	symbols          *symbolInterner
	signatures       *signatureInterner
	byType           map[*checker.Type]TypeID
	byID             map[TypeID]int
	types            []TypeRecord
	limitIDs         map[string]TypeID
	typeNodesUsed    int
	maxDepthObserved int
	budgetTruncated  bool
}

func newTypeInterner(c *checker.Checker, limits BudgetLimits, coreComposite bool) *typeInterner {
	return &typeInterner{
		checker:       c,
		limits:        limits,
		coreComposite: coreComposite,
		byType:        make(map[*checker.Type]TypeID),
		byID:          make(map[TypeID]int),
		limitIDs:      make(map[string]TypeID),
	}
}

func (i *typeInterner) intern(t *checker.Type) TypeID {
	return i.internAtDepth(t, 0)
}

func (i *typeInterner) internAtDepth(t *checker.Type, depth int) TypeID {
	if t == nil {
		return ""
	}
	if id, ok := i.byType[t]; ok {
		return id
	}
	if depth > i.maxDepthObserved {
		i.maxDepthObserved = depth
	}
	if depth > i.limits.MaxTypeDepth {
		return i.internLimit(GraphIssueMaxTypeDepth, i.limits.MaxTypeDepth)
	}
	if i.typeNodesUsed >= i.limits.MaxTypeNodes {
		return i.internLimit(GraphIssueMaxTypeNodes, i.limits.MaxTypeNodes)
	}

	id := TypeID(fmt.Sprintf("type:%d", len(i.types)+1))
	i.byType[t] = id
	i.byID[id] = len(i.types)
	i.types = append(i.types, TypeRecord{
		Record:   "type",
		ID:       id,
		State:    EntityStateComplete,
		Complete: true,
	})
	i.typeNodesUsed++
	index := len(i.types) - 1

	record := TypeRecord{
		Record:   "type",
		ID:       id,
		Display:  i.checker.TypeToString(t),
		Flags:    checker.FormatTypeFlags(t.Flags()),
		State:    EntityStateComplete,
		Complete: true,
	}
	flags := t.Flags()

	if flags&checker.TypeFlagsIntrinsic != 0 && t.AsIntrinsicType().IntrinsicName() == "error" {
		record.TypeKind = "error"
		markTypeIncomplete(&record, EntityStateError, GraphIssue{Code: GraphIssueCheckerError})
	} else {
		switch {
		case flags&checker.TypeFlagsAny != 0:
			record.TypeKind = "any"
		case flags&checker.TypeFlagsUnknown != 0:
			record.TypeKind = "unknown"
		case flags&checker.TypeFlagsNever != 0:
			record.TypeKind = "never"
		case flags&checker.TypeFlagsUndefined != 0:
			record.TypeKind = "undefined"
		case flags&checker.TypeFlagsNull != 0:
			record.TypeKind = "null"
		case flags&checker.TypeFlagsVoid != 0:
			record.TypeKind = "void"
		case i.coreComposite && flags&checker.TypeFlagsUniqueESSymbol != 0:
			record.TypeKind = "unique_symbol"
		case flags&checker.TypeFlagsLiteral != 0:
			record.TypeKind = "literal"
			record.Literal = literalValue(t)
		case flags&checker.TypeFlagsString != 0:
			record.TypeKind = "string"
		case flags&checker.TypeFlagsNumber != 0:
			record.TypeKind = "number"
		case flags&checker.TypeFlagsBigInt != 0:
			record.TypeKind = "bigint"
		case flags&checker.TypeFlagsBoolean != 0:
			record.TypeKind = "boolean"
		case flags&checker.TypeFlagsESSymbol != 0:
			record.TypeKind = "symbol"
		case i.coreComposite && flags&checker.TypeFlagsNonPrimitive != 0:
			record.TypeKind = "non_primitive"
		case flags&checker.TypeFlagsUnion != 0:
			record.TypeKind = "union"
			record.Members, record.Complete = i.internMembers(i.compositeMembers(t), depth+1)
			if !record.Complete {
				markTypeIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteType})
			}
		case flags&checker.TypeFlagsIntersection != 0:
			record.TypeKind = "intersection"
			record.Members, record.Complete = i.internMembers(i.compositeMembers(t), depth+1)
			if !record.Complete {
				markTypeIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteType})
			}
		case flags&checker.TypeFlagsTypeParameter != 0:
			record.TypeKind = "type_parameter"
			if i.coreComposite {
				if t.AsTypeParameter().IsThisType() {
					record.TypeKind = "this"
				}
				i.internTypeParameterEdges(&record, t, depth)
			} else if constraint := i.checker.GetBaseConstraintOfType(t); constraint != nil {
				record.Constraint = i.internAtDepth(constraint, depth+1)
				i.markReferencedIncomplete(&record, record.Constraint)
			}
		case flags&checker.TypeFlagsObject != 0:
			coreObjectShape := false
			collectionShape := i.checker.IsArrayType(t) || checker.IsTupleType(t)
			switch {
			case i.coreComposite && i.checker.IsArrayType(t):
				record.TypeKind = "array"
				record.Array = &ArrayTypeDetails{Readonly: isReadonlyArray(t)}
				i.internReferenceEdges(&record, t, depth)
				coreObjectShape = true
			case i.coreComposite && checker.IsTupleType(t):
				record.TypeKind = "tuple"
				i.internReferenceEdges(&record, t, depth)
				record.Tuple = tupleDetails(t)
				coreObjectShape = true
			case i.coreComposite && t.ObjectFlags()&checker.ObjectFlagsReference != 0:
				record.TypeKind = "reference"
				i.internReferenceEdges(&record, t, depth)
				coreObjectShape = true
			default:
				record.TypeKind = "object"
				if structured := t.AsStructuredType(); structured != nil && len(structured.CallSignatures()) != 0 {
					record.TypeKind = "callable"
				}
			}
			if i.references && !collectionShape {
				i.internObjectEdges(&record, t, depth)
			} else if !coreObjectShape {
				markTypeIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueUnsupportedStructure})
			}
		default:
			record.TypeKind = "opaque"
			markTypeIncomplete(&record, EntityStateUnsupported, GraphIssue{Code: GraphIssueUnsupportedTypeForm})
		}
	}

	i.types[index] = record
	return id
}

func (i *typeInterner) internObjectEdges(record *TypeRecord, t *checker.Type, depth int) {
	nextDepth := depth + 1
	record.Symbol = i.symbols.internAtDepth(t.Symbol(), nextDepth)
	properties := slices.Clone(i.checker.GetPropertiesOfType(t))
	slices.SortStableFunc(properties, func(left, right *ast.Symbol) int {
		return cmp.Compare(ast.EscapeSymbolName(ast.SymbolName(left)), ast.EscapeSymbolName(ast.SymbolName(right)))
	})
	for _, property := range properties {
		record.Properties = append(record.Properties, i.symbols.internAtDepth(property, nextDepth))
	}

	callSignatures := i.checker.GetSignaturesOfType(t, checker.SignatureKindCall)
	constructSignatures := i.checker.GetSignaturesOfType(t, checker.SignatureKindConstruct)
	indexSignatures := slices.Clone(i.checker.GetIndexInfosOfType(t))
	slices.SortStableFunc(indexSignatures, func(left, right *checker.IndexInfo) int {
		return cmp.Compare(i.checker.TypeToString(left.KeyType()), i.checker.TypeToString(right.KeyType()))
	})
	if !i.signatureGraph {
		if len(callSignatures) != 0 || len(constructSignatures) != 0 || len(indexSignatures) != 0 {
			markTypeIncomplete(record, EntityStateTruncated, GraphIssue{Code: GraphIssueUnsupportedStructure})
		}
		return
	}
	for _, signature := range callSignatures {
		record.CallSignatures = append(record.CallSignatures, i.signatures.intern(signature, "call", nextDepth))
	}
	for _, signature := range constructSignatures {
		record.ConstructSignatures = append(record.ConstructSignatures, i.signatures.intern(signature, "construct", nextDepth))
	}
	for _, signature := range indexSignatures {
		record.IndexSignatures = append(record.IndexSignatures, i.signatures.internIndex(signature, nextDepth))
	}
}

func (i *typeInterner) compositeMembers(t *checker.Type) []*checker.Type {
	members := t.Types()
	if !i.coreComposite {
		return members
	}
	members = slices.Clone(members)
	// Union and intersection membership is unordered semantically. Sorting by a
	// protocol category and stable checker display prevents source spelling from
	// leaking into snapshot IDs while avoiding compiler numeric IDs.
	slices.SortStableFunc(members, func(left, right *checker.Type) int {
		return cmp.Compare(i.typeSortKey(left), i.typeSortKey(right))
	})
	return members
}

func (i *typeInterner) typeSortKey(t *checker.Type) string {
	flags := t.Flags()
	kind := "opaque"
	switch {
	case flags&checker.TypeFlagsAny != 0:
		kind = "any"
	case flags&checker.TypeFlagsUnknown != 0:
		kind = "unknown"
	case flags&checker.TypeFlagsNever != 0:
		kind = "never"
	case flags&checker.TypeFlagsUndefined != 0:
		kind = "undefined"
	case flags&checker.TypeFlagsNull != 0:
		kind = "null"
	case flags&checker.TypeFlagsVoid != 0:
		kind = "void"
	case flags&checker.TypeFlagsUniqueESSymbol != 0:
		kind = "unique_symbol"
	case flags&checker.TypeFlagsLiteral != 0:
		kind = "literal"
	case flags&checker.TypeFlagsString != 0:
		kind = "string"
	case flags&checker.TypeFlagsNumber != 0:
		kind = "number"
	case flags&checker.TypeFlagsBigInt != 0:
		kind = "bigint"
	case flags&checker.TypeFlagsBoolean != 0:
		kind = "boolean"
	case flags&checker.TypeFlagsESSymbol != 0:
		kind = "symbol"
	case flags&checker.TypeFlagsNonPrimitive != 0:
		kind = "non_primitive"
	case flags&checker.TypeFlagsUnion != 0:
		kind = "union"
	case flags&checker.TypeFlagsIntersection != 0:
		kind = "intersection"
	case flags&checker.TypeFlagsTypeParameter != 0:
		kind = "type_parameter"
	case flags&checker.TypeFlagsObject != 0:
		kind = "object"
	}
	return kind + "\x00" + i.checker.TypeToString(t)
}

func (i *typeInterner) internReferenceEdges(record *TypeRecord, t *checker.Type, depth int) {
	target := t.Target()
	if target == t {
		record.Target = record.ID
	} else {
		record.Target = i.internAtDepth(target, depth+1)
	}
	record.TypeArguments, _ = i.internMembers(i.checker.GetTypeArguments(t), depth+1)
	i.markReferencedIncomplete(record, append([]TypeID{record.Target}, record.TypeArguments...)...)
}

func (i *typeInterner) internTypeParameterEdges(record *TypeRecord, t *checker.Type, depth int) {
	if target := t.Target(); target != nil {
		if target == t {
			record.Target = record.ID
		} else {
			record.Target = i.internAtDepth(target, depth+1)
		}
	}
	if constraint := i.checker.GetConstraintOfTypeParameter(t); constraint != nil {
		record.Constraint = i.internAtDepth(constraint, depth+1)
	}
	if defaultType := i.checker.GetDefaultFromTypeParameter(t); defaultType != nil {
		record.Default = i.internAtDepth(defaultType, depth+1)
	}
	i.markReferencedIncomplete(record, record.Target, record.Constraint, record.Default)
}

func (i *typeInterner) markReferencedIncomplete(record *TypeRecord, ids ...TypeID) {
	for _, id := range ids {
		if id != "" && id != record.ID && !i.complete(id) {
			markTypeIncomplete(record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteType})
			return
		}
	}
}

func isReadonlyArray(t *checker.Type) bool {
	target := t.Target()
	return target != nil && target.Symbol() != nil && target.Symbol().Name == "ReadonlyArray"
}

func tupleDetails(t *checker.Type) *TupleTypeDetails {
	target := t.TargetTupleType()
	elements := make([]TupleElementDetails, 0, len(target.ElementInfos()))
	for _, info := range target.ElementInfos() {
		element := TupleElementDetails{Kind: tupleElementKind(info.TupleElementFlags())}
		if declaration := info.LabeledDeclaration(); declaration != nil {
			if name := declaration.Name(); name != nil && ast.IsIdentifier(name) {
				element.Label = name.Text()
			}
		}
		elements = append(elements, element)
	}
	return &TupleTypeDetails{Readonly: target.IsReadonly(), Elements: elements}
}

func tupleElementKind(flags checker.ElementFlags) string {
	switch {
	case flags&checker.ElementFlagsVariadic != 0:
		return "variadic"
	case flags&checker.ElementFlagsRest != 0:
		return "rest"
	case flags&checker.ElementFlagsOptional != 0:
		return "optional"
	default:
		return "required"
	}
}

func (i *typeInterner) internMembers(types []*checker.Type, depth int) ([]TypeID, bool) {
	members := make([]TypeID, 0, len(types))
	complete := true
	for _, member := range types {
		id := i.internAtDepth(member, depth)
		members = append(members, id)
		complete = complete && i.complete(id)
	}
	return members, complete
}

func (i *typeInterner) internLimit(code string, limit int) TypeID {
	if id := i.limitIDs[code]; id != "" {
		return id
	}
	id := TypeID(fmt.Sprintf("type:%d", len(i.types)+1))
	i.limitIDs[code] = id
	i.budgetTruncated = true
	i.byID[id] = len(i.types)
	i.types = append(i.types, TypeRecord{
		Record:    "type",
		ID:        id,
		TypeKind:  "truncated",
		Display:   "<" + code + ">",
		Flags:     []string{"None"},
		State:     EntityStateTruncated,
		Issues:    []GraphIssue{{Code: code, Limit: limit}},
		Truncated: true,
	})
	return id
}

func (i *typeInterner) complete(id TypeID) bool {
	if id == "" {
		return true
	}
	index, ok := i.byID[id]
	return ok && i.types[index].Complete
}

func (i *typeInterner) truncated(id TypeID) bool {
	if id == "" {
		return false
	}
	index, ok := i.byID[id]
	return ok && i.types[index].State == EntityStateTruncated
}

func (i *typeInterner) budgetReport() BudgetReport {
	return BudgetReport{
		Limits:               i.limits,
		TypeNodesUsed:        i.typeNodesUsed,
		MaxTypeDepthObserved: i.maxDepthObserved,
		Truncated:            i.budgetTruncated,
	}
}

func markTypeIncomplete(record *TypeRecord, state string, issue GraphIssue) {
	record.State = state
	record.Issues = appendGraphIssue(record.Issues, issue.Code)
	record.Complete = false
	record.Truncated = state == EntityStateTruncated
}

func literalValue(t *checker.Type) *LiteralValue {
	value := t.AsLiteralType().Value()
	kind := "unknown"
	if t.Flags()&checker.TypeFlagsEnumLiteral != 0 {
		kind = "enum"
	} else {
		switch value.(type) {
		case string:
			kind = "string"
		case bool:
			kind = "boolean"
		default:
			switch {
			case t.Flags()&checker.TypeFlagsNumberLiteral != 0:
				kind = "number"
			case t.Flags()&checker.TypeFlagsBigIntLiteral != 0:
				kind = "bigint"
			}
		}
	}
	return &LiteralValue{Kind: kind, Value: fmt.Sprint(value)}
}

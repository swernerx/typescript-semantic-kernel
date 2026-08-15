package tsfacts

import (
	"fmt"

	"github.com/microsoft/typescript-go/internal/checker"
)

type typeInterner struct {
	checker          *checker.Checker
	limits           BudgetLimits
	byType           map[*checker.Type]TypeID
	types            []TypeRecord
	limitIDs         map[string]TypeID
	typeNodesUsed    int
	maxDepthObserved int
	budgetTruncated  bool
}

func newTypeInterner(c *checker.Checker, limits BudgetLimits) *typeInterner {
	return &typeInterner{
		checker:  c,
		limits:   limits,
		byType:   make(map[*checker.Type]TypeID),
		limitIDs: make(map[string]TypeID),
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
	i.types = append(i.types, TypeRecord{Record: "type", ID: id})
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
		case flags&checker.TypeFlagsUnion != 0:
			record.TypeKind = "union"
			record.Members, record.Complete = i.internMembers(t.Types(), depth+1)
			if !record.Complete {
				markTypeIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteType})
			}
		case flags&checker.TypeFlagsIntersection != 0:
			record.TypeKind = "intersection"
			record.Members, record.Complete = i.internMembers(t.Types(), depth+1)
			if !record.Complete {
				markTypeIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteType})
			}
		case flags&checker.TypeFlagsTypeParameter != 0:
			record.TypeKind = "type_parameter"
			if constraint := i.checker.GetBaseConstraintOfType(t); constraint != nil {
				record.Constraint = i.internAtDepth(constraint, depth+1)
				record.Complete = i.complete(record.Constraint)
				if !record.Complete {
					markTypeIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteType})
				}
			}
		case flags&checker.TypeFlagsObject != 0:
			record.TypeKind = "object"
			if structured := t.AsStructuredType(); structured != nil && len(structured.CallSignatures()) != 0 {
				record.TypeKind = "callable"
			}
			markTypeIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueUnsupportedStructure})
		default:
			record.TypeKind = "opaque"
			markTypeIncomplete(&record, EntityStateUnsupported, GraphIssue{Code: GraphIssueUnsupportedTypeForm})
		}
	}

	i.types[index] = record
	return id
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
	for index := range i.types {
		if i.types[index].ID == id {
			return i.types[index].Complete
		}
	}
	return false
}

func (i *typeInterner) truncated(id TypeID) bool {
	if id == "" {
		return false
	}
	for index := range i.types {
		if i.types[index].ID == id {
			return i.types[index].State == EntityStateTruncated
		}
	}
	return false
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
	record.Issues = append(record.Issues, issue)
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

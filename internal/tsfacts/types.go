package tsfacts

import (
	"fmt"

	"github.com/microsoft/typescript-go/internal/checker"
)

const (
	maxTypeDepth = 32
	maxTypeNodes = 4096
)

type typeInterner struct {
	checker *checker.Checker
	byType  map[*checker.Type]TypeID
	types   []TypeRecord
	limitID TypeID
}

func newTypeInterner(c *checker.Checker) *typeInterner {
	return &typeInterner{
		checker: c,
		byType:  make(map[*checker.Type]TypeID),
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
	if depth > maxTypeDepth || len(i.types) >= maxTypeNodes {
		return i.internLimit()
	}

	id := TypeID(fmt.Sprintf("type:%d", len(i.types)+1))
	i.byType[t] = id
	i.types = append(i.types, TypeRecord{Record: "type", ID: id})
	index := len(i.types) - 1

	record := TypeRecord{
		Record:   "type",
		ID:       id,
		Display:  i.checker.TypeToString(t),
		Flags:    checker.FormatTypeFlags(t.Flags()),
		Complete: true,
	}
	flags := t.Flags()

	if flags&checker.TypeFlagsIntrinsic != 0 && t.AsIntrinsicType().IntrinsicName() == "error" {
		record.TypeKind = "error"
		record.Complete = false
		record.Truncated = true
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
			record.Truncated = !record.Complete
		case flags&checker.TypeFlagsIntersection != 0:
			record.TypeKind = "intersection"
			record.Members, record.Complete = i.internMembers(t.Types(), depth+1)
			record.Truncated = !record.Complete
		case flags&checker.TypeFlagsTypeParameter != 0:
			record.TypeKind = "type_parameter"
			if constraint := i.checker.GetBaseConstraintOfType(t); constraint != nil {
				record.Constraint = i.internAtDepth(constraint, depth+1)
				record.Complete = i.complete(record.Constraint)
				record.Truncated = !record.Complete
			}
		case flags&checker.TypeFlagsObject != 0:
			record.TypeKind = "object"
			if structured := t.AsStructuredType(); structured != nil && len(structured.CallSignatures()) != 0 {
				record.TypeKind = "callable"
			}
			record.Complete = false
			record.Truncated = true
		default:
			record.TypeKind = "opaque"
			record.Complete = false
			record.Truncated = true
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

func (i *typeInterner) internLimit() TypeID {
	if i.limitID != "" {
		return i.limitID
	}
	i.limitID = TypeID(fmt.Sprintf("type:%d", len(i.types)+1))
	i.types = append(i.types, TypeRecord{
		Record:    "type",
		ID:        i.limitID,
		TypeKind:  "truncated",
		Display:   "<serialization limit>",
		Flags:     []string{"None"},
		Truncated: true,
	})
	return i.limitID
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

package tsfacts

import (
	"errors"
	"fmt"
	"strings"
)

var knownTypeKinds = map[string]struct{}{
	"any": {}, "bigint": {}, "boolean": {}, "callable": {}, "error": {},
	"intersection": {}, "literal": {}, "never": {}, "null": {}, "number": {},
	"object": {}, "opaque": {}, "string": {}, "symbol": {}, "truncated": {}, "unsupported": {},
	"type_parameter": {}, "undefined": {}, "union": {}, "unknown": {}, "void": {},
}

var knownSignatureKinds = map[string]struct{}{
	"call": {}, "construct": {}, "index": {},
}

func ValidateResult(result *Result) error {
	if result == nil {
		return errors.New("result is required")
	}
	if result.Header.Record != "header" {
		return fmt.Errorf("header record must be %q", "header")
	}
	if result.Header.SchemaVersion != SchemaVersion {
		return fmt.Errorf("header schemaVersion %d does not match %d", result.Header.SchemaVersion, SchemaVersion)
	}

	files := make(map[string]struct{}, len(result.Files))
	for index, file := range result.Files {
		if file.Record != "file" {
			return fmt.Errorf("files[%d] record must be %q", index, "file")
		}
		if err := addID(files, file.ID, "file", index); err != nil {
			return err
		}
	}

	types := make(map[TypeID]TypeRecord, len(result.Types))
	for index, record := range result.Types {
		if record.Record != "type" {
			return fmt.Errorf("types[%d] record must be %q", index, "type")
		}
		if err := addTypedID(types, record.ID, "type:", "type", index, record); err != nil {
			return err
		}
		if _, ok := knownTypeKinds[record.TypeKind]; !ok {
			return fmt.Errorf("type %q has unknown typeKind %q", record.ID, record.TypeKind)
		}
		if record.Complete == record.Truncated {
			return fmt.Errorf("type %q must be exactly one of complete or truncated", record.ID)
		}
	}

	declarations := make(map[DeclarationID]DeclarationRecord, len(result.Declarations))
	for index, record := range result.Declarations {
		if record.Record != "declaration" {
			return fmt.Errorf("declarations[%d] record must be %q", index, "declaration")
		}
		if err := addTypedID(declarations, record.ID, "declaration:", "declaration", index, record); err != nil {
			return err
		}
		if _, ok := files[record.File]; !ok {
			return fmt.Errorf("declaration %q references missing file %q", record.ID, record.File)
		}
	}

	symbols := make(map[SymbolID]SymbolRecord, len(result.Symbols))
	for index, record := range result.Symbols {
		if record.Record != "symbol" {
			return fmt.Errorf("symbols[%d] record must be %q", index, "symbol")
		}
		if err := addTypedID(symbols, record.ID, "symbol:", "symbol", index, record); err != nil {
			return err
		}
		if record.Complete == record.Truncated {
			return fmt.Errorf("symbol %q must be exactly one of complete or truncated", record.ID)
		}
	}

	signatures := make(map[SignatureID]SignatureRecord, len(result.Signatures))
	for index, record := range result.Signatures {
		if record.Record != "signature" {
			return fmt.Errorf("signatures[%d] record must be %q", index, "signature")
		}
		if err := addTypedID(signatures, record.ID, "signature:", "signature", index, record); err != nil {
			return err
		}
		if _, ok := knownSignatureKinds[record.SignatureKind]; !ok {
			return fmt.Errorf("signature %q has unknown signatureKind %q", record.ID, record.SignatureKind)
		}
		if record.ReturnType == "" {
			return fmt.Errorf("signature %q requires returnType", record.ID)
		}
		if record.Complete == record.Truncated {
			return fmt.Errorf("signature %q must be exactly one of complete or truncated", record.ID)
		}
	}

	for _, record := range result.Types {
		owner := "type " + string(record.ID)
		typeEdges := appendTypeIDs(record.Members, record.Target, record.TypeArguments, record.Constraint, record.Default)
		symbolEdges := appendSymbolIDs(record.Properties, record.Symbol)
		if err := requireTypes(types, owner, typeEdges); err != nil {
			return err
		}
		if err := requireSymbols(symbols, owner, symbolEdges); err != nil {
			return err
		}
		if err := requireSignatures(signatures, owner, record.CallSignatures, record.ConstructSignatures, record.IndexSignatures); err != nil {
			return err
		}
		if record.Complete {
			if err := requireCompleteTypes(types, owner, typeEdges); err != nil {
				return err
			}
			if err := requireCompleteSymbols(symbols, owner, symbolEdges); err != nil {
				return err
			}
			if err := requireCompleteSignatures(signatures, owner, record.CallSignatures, record.ConstructSignatures, record.IndexSignatures); err != nil {
				return err
			}
		}
	}
	for _, record := range result.Symbols {
		owner := "symbol " + string(record.ID)
		symbolEdges := appendSymbolIDs(record.Members, record.AliasedSymbol)
		typeEdges := []TypeID{record.Type, record.DeclaredType}
		if err := requireDeclarations(declarations, owner, record.Declarations); err != nil {
			return err
		}
		if err := requireSymbols(symbols, owner, symbolEdges); err != nil {
			return err
		}
		if err := requireTypes(types, owner, typeEdges); err != nil {
			return err
		}
		if record.Complete {
			if err := requireCompleteSymbols(symbols, owner, symbolEdges); err != nil {
				return err
			}
			if err := requireCompleteTypes(types, owner, typeEdges); err != nil {
				return err
			}
		}
	}
	for _, record := range result.Signatures {
		owner := "signature " + string(record.ID)
		typeEdges := appendTypeIDs(record.TypeParameters, record.ThisType, nil, record.ReturnType)
		if err := requireDeclarations(declarations, owner, []DeclarationID{record.Declaration}); err != nil {
			return err
		}
		if err := requireTypes(types, owner, typeEdges); err != nil {
			return err
		}
		if err := requireSymbols(symbols, owner, record.Parameters); err != nil {
			return err
		}
		if record.Complete {
			if err := requireCompleteTypes(types, owner, typeEdges); err != nil {
				return err
			}
			if err := requireCompleteSymbols(symbols, owner, record.Parameters); err != nil {
				return err
			}
		}
	}
	for index, fact := range result.Facts {
		if fact.Record != "fact" {
			return fmt.Errorf("facts[%d] record must be %q", index, "fact")
		}
		owner := fmt.Sprintf("fact[%d]", index)
		if _, ok := files[fact.File]; !ok {
			return fmt.Errorf("%s references missing file %q", owner, fact.File)
		}
		if fact.ActualType == "" || fact.ActualType != fact.TypeAtLocation {
			return fmt.Errorf("%s actualType must equal required typeAtLocation", owner)
		}
		if err := requireTypes(types, owner, []TypeID{
			fact.ActualType, fact.AnnotationType, fact.InferredType, fact.ContextualType,
			fact.WidenedType, fact.ApparentType, fact.DeclaredType, fact.NarrowedType, fact.ConstraintType,
		}); err != nil {
			return err
		}
		if err := requireSymbols(symbols, owner, []SymbolID{fact.Symbol}); err != nil {
			return err
		}
		if err := requireDeclarations(declarations, owner, fact.Declarations); err != nil {
			return err
		}
		if fact.Complete != (!fact.Recovered && !fact.Truncated) {
			return fmt.Errorf("%s completeness is inconsistent with recovered and truncated", owner)
		}
		if fact.Complete {
			if err := requireCompleteTypes(types, owner, []TypeID{
				fact.ActualType, fact.AnnotationType, fact.InferredType, fact.ContextualType,
				fact.WidenedType, fact.ApparentType, fact.DeclaredType, fact.NarrowedType, fact.ConstraintType,
			}); err != nil {
				return err
			}
			if err := requireCompleteSymbols(symbols, owner, []SymbolID{fact.Symbol}); err != nil {
				return err
			}
		}
	}
	return nil
}

func addID[T any](ids map[string]T, id string, kind string, index int) error {
	if id == "" {
		return fmt.Errorf("%ss[%d] requires id", kind, index)
	}
	if _, ok := ids[id]; ok {
		return fmt.Errorf("duplicate %s id %q", kind, id)
	}
	var zero T
	ids[id] = zero
	return nil
}

func addTypedID[ID ~string, Record any](ids map[ID]Record, id ID, prefix string, kind string, index int, record Record) error {
	if !strings.HasPrefix(string(id), prefix) || len(id) == len(prefix) {
		return fmt.Errorf("%ss[%d] has invalid id %q; expected %s…", kind, index, id, prefix)
	}
	if _, ok := ids[id]; ok {
		return fmt.Errorf("duplicate %s id %q", kind, id)
	}
	ids[id] = record
	return nil
}

func appendTypeIDs(before []TypeID, single TypeID, after []TypeID, final ...TypeID) []TypeID {
	result := make([]TypeID, 0, len(before)+len(after)+1+len(final))
	result = append(result, before...)
	result = append(result, single)
	result = append(result, after...)
	return append(result, final...)
}

func appendSymbolIDs(before []SymbolID, final ...SymbolID) []SymbolID {
	result := append([]SymbolID(nil), before...)
	return append(result, final...)
}

func requireTypes(known map[TypeID]TypeRecord, owner string, ids []TypeID) error {
	for _, id := range ids {
		if id == "" {
			continue
		}
		if _, ok := known[id]; !ok {
			return fmt.Errorf("%s references missing type %q", owner, id)
		}
	}
	return nil
}

func requireSymbols(known map[SymbolID]SymbolRecord, owner string, ids []SymbolID) error {
	for _, id := range ids {
		if id == "" {
			continue
		}
		if _, ok := known[id]; !ok {
			return fmt.Errorf("%s references missing symbol %q", owner, id)
		}
	}
	return nil
}

func requireDeclarations(known map[DeclarationID]DeclarationRecord, owner string, ids []DeclarationID) error {
	for _, id := range ids {
		if id == "" {
			continue
		}
		if _, ok := known[id]; !ok {
			return fmt.Errorf("%s references missing declaration %q", owner, id)
		}
	}
	return nil
}

func requireSignatures(known map[SignatureID]SignatureRecord, owner string, groups ...[]SignatureID) error {
	for _, ids := range groups {
		for _, id := range ids {
			if id == "" {
				continue
			}
			if _, ok := known[id]; !ok {
				return fmt.Errorf("%s references missing signature %q", owner, id)
			}
		}
	}
	return nil
}

func requireCompleteTypes(known map[TypeID]TypeRecord, owner string, ids []TypeID) error {
	for _, id := range ids {
		if id != "" && !known[id].Complete {
			return fmt.Errorf("complete %s references truncated type %q", owner, id)
		}
	}
	return nil
}

func requireCompleteSymbols(known map[SymbolID]SymbolRecord, owner string, ids []SymbolID) error {
	for _, id := range ids {
		if id != "" && !known[id].Complete {
			return fmt.Errorf("complete %s references truncated symbol %q", owner, id)
		}
	}
	return nil
}

func requireCompleteSignatures(known map[SignatureID]SignatureRecord, owner string, groups ...[]SignatureID) error {
	for _, ids := range groups {
		for _, id := range ids {
			if id != "" && !known[id].Complete {
				return fmt.Errorf("complete %s references truncated signature %q", owner, id)
			}
		}
	}
	return nil
}

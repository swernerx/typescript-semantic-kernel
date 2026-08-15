package semanticfacts

import (
	"errors"
	"fmt"
	"slices"
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
	if err := validateCapabilities(result.Header.Capabilities); err != nil {
		return err
	}
	limits, err := normalizeBudgetLimits(result.Header.Budgets.Limits)
	if err != nil {
		return fmt.Errorf("header budgets: %w", err)
	}
	if limits != result.Header.Budgets.Limits {
		return errors.New("header budgets must contain normalized limits")
	}
	if result.Header.Budgets.TypeNodesUsed < 0 || result.Header.Budgets.TypeNodesUsed > limits.MaxTypeNodes {
		return fmt.Errorf("header typeNodesUsed %d exceeds limit %d", result.Header.Budgets.TypeNodesUsed, limits.MaxTypeNodes)
	}
	if result.Header.Budgets.MaxTypeDepthObserved < 0 {
		return errors.New("header maxTypeDepthObserved must not be negative")
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
		if err := validateEntityState("type", string(record.ID), record.State, record.Issues, record.Complete, record.Truncated); err != nil {
			return err
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
		if err := validateEntityState("symbol", string(record.ID), record.State, record.Issues, record.Complete, record.Truncated); err != nil {
			return err
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
		if err := validateEntityState("signature", string(record.ID), record.State, record.Issues, record.Complete, record.Truncated); err != nil {
			return err
		}
	}
	budgetSentinels := 0
	budgetIssueIDs := make(map[string]TypeID, 2)
	for _, record := range result.Types {
		budgetSentinel := false
		budgetCode := ""
		for _, issue := range record.Issues {
			switch issue.Code {
			case GraphIssueMaxTypeDepth:
				if issue.Limit != limits.MaxTypeDepth {
					return fmt.Errorf("type %q max-type-depth issue has limit %d; expected %d", record.ID, issue.Limit, limits.MaxTypeDepth)
				}
				if record.TypeKind != "truncated" {
					return fmt.Errorf("type %q budget issue requires truncated typeKind", record.ID)
				}
				budgetSentinel = true
				budgetCode = issue.Code
			case GraphIssueMaxTypeNodes:
				if issue.Limit != limits.MaxTypeNodes {
					return fmt.Errorf("type %q max-type-nodes issue has limit %d; expected %d", record.ID, issue.Limit, limits.MaxTypeNodes)
				}
				if record.TypeKind != "truncated" {
					return fmt.Errorf("type %q budget issue requires truncated typeKind", record.ID)
				}
				if budgetSentinel {
					return fmt.Errorf("type %q must not combine budget sentinel issues", record.ID)
				}
				budgetSentinel = true
				budgetCode = issue.Code
			default:
				if issue.Limit != 0 {
					return fmt.Errorf("type %q issue %q must not contain limit", record.ID, issue.Code)
				}
			}
		}
		if budgetSentinel {
			if record.State != EntityStateTruncated {
				return fmt.Errorf("type %q budget sentinel must have truncated state", record.ID)
			}
			if previous := budgetIssueIDs[budgetCode]; previous != "" {
				return fmt.Errorf("types %q and %q duplicate budget sentinel %q", previous, record.ID, budgetCode)
			}
			budgetIssueIDs[budgetCode] = record.ID
			budgetSentinels++
		}
	}
	if result.Header.Budgets.TypeNodesUsed != len(result.Types)-budgetSentinels {
		return fmt.Errorf("header typeNodesUsed %d does not match %d non-sentinel type nodes", result.Header.Budgets.TypeNodesUsed, len(result.Types)-budgetSentinels)
	}
	if result.Header.Budgets.Truncated != (budgetSentinels != 0) {
		return errors.New("header budget truncation does not match graph sentinels")
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
		if fact.TypeViewStates.Actual != TypeViewAvailable {
			return fmt.Errorf("%s actual type view must be %q", owner, TypeViewAvailable)
		}
		if err := validateOptionalTypeView(owner, "contextual", fact.ContextualType, fact.TypeViewStates.Contextual); err != nil {
			return err
		}
		if err := validateOptionalTypeView(owner, "widened", fact.WidenedType, fact.TypeViewStates.Widened); err != nil {
			return err
		}
		if err := validateOptionalTypeView(owner, "apparent", fact.ApparentType, fact.TypeViewStates.Apparent); err != nil {
			return err
		}
		if err := validateOptionalTypeView(owner, "declared", fact.DeclaredType, fact.TypeViewStates.Declared); err != nil {
			return err
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
		factTypeIDs := []TypeID{
			fact.ActualType, fact.AnnotationType, fact.InferredType, fact.ContextualType,
			fact.WidenedType, fact.ApparentType, fact.DeclaredType, fact.NarrowedType, fact.ConstraintType,
		}
		referencesTruncated := anyTruncatedType(types, factTypeIDs) || fact.Symbol != "" && symbols[fact.Symbol].Truncated
		if fact.Truncated != referencesTruncated {
			return fmt.Errorf("%s truncation does not match referenced graph entities", owner)
		}
		referencesComplete := allCompleteTypes(types, factTypeIDs) && (fact.Symbol == "" || symbols[fact.Symbol].Complete)
		if fact.Complete != (!fact.Recovered && referencesComplete) {
			return fmt.Errorf("%s completeness does not match recovery and referenced graph entities", owner)
		}
		if fact.Complete {
			if err := requireCompleteTypes(types, owner, factTypeIDs); err != nil {
				return err
			}
			if err := requireCompleteSymbols(symbols, owner, []SymbolID{fact.Symbol}); err != nil {
				return err
			}
		}
	}
	return nil
}

func validateEntityState(kind string, id string, state string, issues []GraphIssue, complete bool, truncated bool) error {
	if state != EntityStateComplete && state != EntityStateTruncated && state != EntityStateUnsupported && state != EntityStateError {
		return fmt.Errorf("%s %q has unknown state %q", kind, id, state)
	}
	if complete != (state == EntityStateComplete) {
		return fmt.Errorf("%s %q complete flag does not match state %q", kind, id, state)
	}
	if truncated != (state == EntityStateTruncated) {
		return fmt.Errorf("%s %q truncated flag does not match state %q", kind, id, state)
	}
	if state == EntityStateComplete && len(issues) != 0 {
		return fmt.Errorf("complete %s %q must not contain issues", kind, id)
	}
	if state != EntityStateComplete && len(issues) == 0 {
		return fmt.Errorf("incomplete %s %q requires a machine-readable issue", kind, id)
	}
	for index, issue := range issues {
		if issue.Code == "" {
			return fmt.Errorf("%s %q issues[%d] requires code", kind, id, index)
		}
		if index != 0 && issues[index-1].Code >= issue.Code {
			return fmt.Errorf("%s %q issues must be unique and sorted by code", kind, id)
		}
	}
	return nil
}

func validateCapabilities(capabilities []string) error {
	if !slices.IsSorted(capabilities) {
		return errors.New("header capabilities must be sorted")
	}
	for index, capability := range capabilities {
		if capability == "" {
			return fmt.Errorf("header capabilities[%d] must not be empty", index)
		}
		if index != 0 && capabilities[index-1] == capability {
			return errors.New("header capabilities must be unique")
		}
	}
	for _, required := range supportedCapabilities {
		if _, present := slices.BinarySearch(capabilities, required); !present {
			return fmt.Errorf("header capabilities omit schema-v1 capability %q", required)
		}
	}
	return nil
}

func anyTruncatedType(known map[TypeID]TypeRecord, ids []TypeID) bool {
	for _, id := range ids {
		if id != "" && known[id].Truncated {
			return true
		}
	}
	return false
}

func allCompleteTypes(known map[TypeID]TypeRecord, ids []TypeID) bool {
	for _, id := range ids {
		if id != "" && !known[id].Complete {
			return false
		}
	}
	return true
}

func validateOptionalTypeView(owner string, view string, id TypeID, state string) error {
	if state != TypeViewAvailable && state != TypeViewSameAsActual && state != TypeViewInapplicable && state != TypeViewUnavailable {
		return fmt.Errorf("%s %s type view has unknown state %q", owner, view, state)
	}
	if state == TypeViewAvailable && id == "" {
		return fmt.Errorf("%s %s type view is available without a root", owner, view)
	}
	if state != TypeViewAvailable && id != "" {
		return fmt.Errorf("%s %s type view state %q must omit its root", owner, view, state)
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

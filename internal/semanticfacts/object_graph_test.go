package semanticfacts_test

import (
	"bytes"
	"slices"
	"testing"

	tsfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	transport "github.com/microsoft/typescript-go/internal/tsfacts"
	"gotest.tools/v3/assert"
)

func TestAnalyzeExportsRecursiveObjectPropertiesAndSymbols(t *testing.T) {
	t.Parallel()
	const source = `
interface KernelNode {
    label: string;
    next?: KernelNode;
}
declare const node: KernelNode;
node;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		RequiredCapabilities: []string{
			tsfacts.CapabilityCoreCompositeTypes,
			tsfacts.CapabilityGraphReferences,
		},
		Project:    "tsconfig.json",
		Selections: []tsfacts.Selection{selectionAt(source, "node", 1)},
	})
	assert.NilError(t, tsfacts.ValidateResult(result))

	fact := result.Facts[0]
	root := typeByID(t, result, fact.ActualType)
	assert.Equal(t, root.TypeKind, "object")
	assert.Assert(t, root.Complete)
	assert.Assert(t, root.Symbol != "")
	assert.Equal(t, len(root.Properties), 2)

	propertyNames := make([]string, 0, len(root.Properties))
	var next tsfacts.SymbolRecord
	for _, propertyID := range root.Properties {
		property := symbolByID(t, result, propertyID)
		propertyNames = append(propertyNames, property.Name)
		assert.Assert(t, property.Type != "")
		assert.Assert(t, len(property.Declarations) != 0)
		if property.Name == "next" {
			next = property
		}
	}
	assert.DeepEqual(t, propertyNames, []string{"label", "next"})
	assert.Assert(t, next.ID != "")
	nextType := typeByID(t, result, next.Type)
	assert.Equal(t, nextType.TypeKind, "union")
	assert.Assert(t, slices.Contains(nextType.Members, root.ID))

	interfaceSymbol := symbolByID(t, result, root.Symbol)
	assert.Equal(t, interfaceSymbol.Name, "KernelNode")
	assert.Equal(t, interfaceSymbol.DeclaredType, root.ID)
	assert.DeepEqual(t, interfaceSymbol.Members, root.Properties)
	variable := symbolByID(t, result, fact.Symbol)
	assert.Equal(t, variable.Type, root.ID)
	assert.Assert(t, fact.Complete)
}

func TestAnalyzeExportsOverloadsIndexesAndGenericSignatures(t *testing.T) {
	t.Parallel()
	const source = `
interface Array<T> { length: number }
interface Box<T> { value: T }
interface Callable<T> {
    (value: T): Box<T>;
    (value: number, ...labels: string[]): Box<number>;
    new <U extends string = string>(value: U): Box<U>;
    readonly [key: string]: Box<T>;
}
declare const callable: Callable<string>;
declare function identity<T>(value: T): T;
const stringIdentity = identity<string>;
callable;
stringIdentity;
`
	request := tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		RequiredCapabilities: []string{
			tsfacts.CapabilityCoreCompositeTypes,
			tsfacts.CapabilityGraphReferences,
			tsfacts.CapabilityGraphSignatures,
		},
		Project: "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "callable", 1),
			selectionAt(source, "stringIdentity", 1),
		},
	}
	result := analyzeFixture(t, source, request)
	assert.NilError(t, tsfacts.ValidateResult(result))

	callable := typeByID(t, result, result.Facts[0].ActualType)
	assert.Equal(t, callable.TypeKind, "reference")
	assert.Equal(t, len(callable.CallSignatures), 2)
	assert.Equal(t, len(callable.ConstructSignatures), 1)
	assert.Equal(t, len(callable.IndexSignatures), 1)
	assert.Assert(t, callable.CallSignatures[0] != callable.CallSignatures[1])

	foundRestParameter := false
	for _, id := range callable.CallSignatures {
		signature := signatureByID(t, result, id)
		assert.Equal(t, signature.SignatureKind, "call")
		assert.Assert(t, len(signature.Parameters) >= 1)
		assert.Equal(t, signature.MinArgumentCount, 1)
		assert.Assert(t, signature.ReturnType != "")
		parameter := symbolByID(t, result, signature.Parameters[0])
		assert.Assert(t, parameter.Type != "")
		assert.Assert(t, len(parameter.Declarations) != 0)
		if len(signature.Parameters) == 2 {
			foundRestParameter = true
			assert.Equal(t, len(signature.Parameters), 2)
			assert.Assert(t, signature.HasRestParameter)
			restType := typeByID(t, result, symbolByID(t, result, signature.Parameters[1]).Type)
			assert.Equal(t, restType.TypeKind, "array")
			assert.Equal(t, len(restType.Properties), 0)
		}
	}
	assert.Assert(t, foundRestParameter)

	construct := signatureByID(t, result, callable.ConstructSignatures[0])
	assert.Equal(t, construct.SignatureKind, "construct")
	assert.Equal(t, len(construct.TypeParameters), 1)
	typeParameter := typeByID(t, result, construct.TypeParameters[0])
	assert.Assert(t, typeParameter.Constraint != "")
	assert.Assert(t, typeParameter.Default != "")

	index := signatureByID(t, result, callable.IndexSignatures[0])
	assert.Equal(t, index.SignatureKind, "index")
	assert.Equal(t, index.MinArgumentCount, 1)
	assert.Assert(t, index.Readonly)
	assert.Equal(t, typeByID(t, result, index.IndexKeyType).TypeKind, "string")
	assert.Assert(t, index.ReturnType != "")

	instantiated := typeByID(t, result, result.Facts[1].ActualType)
	assert.Equal(t, len(instantiated.CallSignatures), 1)
	instantiatedSignature := signatureByID(t, result, instantiated.CallSignatures[0])
	assert.Assert(t, instantiatedSignature.Target != "")
	assert.Equal(t, len(instantiatedSignature.TypeArguments), 1)
	assert.Equal(t, typeByID(t, result, instantiatedSignature.TypeArguments[0]).TypeKind, "string")
	assert.Assert(t, result.Facts[0].Complete)
	assert.Assert(t, result.Facts[1].Complete)

	repeated := analyzeFixture(t, source, request)
	var firstOutput bytes.Buffer
	var repeatedOutput bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&firstOutput, result))
	assert.NilError(t, transport.WriteJSONLines(&repeatedOutput, repeated))
	assert.Equal(t, firstOutput.String(), repeatedOutput.String())
}

func TestAnalyzeLinksClassInstanceConstructorAndMethodGraphs(t *testing.T) {
	t.Parallel()
	const source = `
class KernelService {
    value: string = "ready";
    read(input: number): string { return this.value; }
}
declare const service: KernelService;
service;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		RequiredCapabilities: []string{
			tsfacts.CapabilityCoreCompositeTypes,
			tsfacts.CapabilityGraphReferences,
			tsfacts.CapabilityGraphSignatures,
		},
		Project:    "tsconfig.json",
		Selections: []tsfacts.Selection{selectionAt(source, "service", 1)},
	})
	assert.NilError(t, tsfacts.ValidateResult(result))

	instance := typeByID(t, result, result.Facts[0].ActualType)
	classSymbol := symbolByID(t, result, instance.Symbol)
	assert.Assert(t, slices.Contains(classSymbol.Roles, "class"))
	assert.Equal(t, classSymbol.DeclaredType, instance.ID)
	assert.Assert(t, classSymbol.Type != "")
	constructor := typeByID(t, result, classSymbol.Type)
	assert.Equal(t, len(constructor.ConstructSignatures), 1)
	constructSignature := signatureByID(t, result, constructor.ConstructSignatures[0])
	assert.Equal(t, constructSignature.ReturnType, instance.ID)

	var method tsfacts.SymbolRecord
	for _, propertyID := range instance.Properties {
		property := symbolByID(t, result, propertyID)
		if property.Name == "read" {
			method = property
		}
	}
	assert.Assert(t, method.ID != "")
	methodType := typeByID(t, result, method.Type)
	assert.Equal(t, len(methodType.CallSignatures), 1)
	methodSignature := signatureByID(t, result, methodType.CallSignatures[0])
	assert.Equal(t, typeByID(t, result, methodSignature.ReturnType).TypeKind, "string")
	assert.Equal(t, len(methodSignature.Parameters), 1)
	assert.Equal(t, typeByID(t, result, symbolByID(t, result, methodSignature.Parameters[0]).Type).TypeKind, "number")
	assert.Assert(t, result.Facts[0].Complete)
}

func TestAnalyzeGatesDeepObjectGraphOnGraphCapabilities(t *testing.T) {
	t.Parallel()
	const source = `
interface Callable { (value: string): string; property: number }
declare const callable: Callable;
callable;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "callable", 1)},
	})

	record := typeByID(t, result, result.Facts[0].ActualType)
	assert.Assert(t, record.TypeKind == "object" || record.TypeKind == "callable")
	assert.Equal(t, record.State, tsfacts.EntityStateTruncated)
	assert.Equal(t, len(record.Properties), 0)
	assert.Equal(t, len(record.CallSignatures), 0)
	symbol := symbolByID(t, result, result.Facts[0].Symbol)
	assert.Equal(t, symbol.Type, tsfacts.TypeID(""))

	negotiated := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityGraphSignatures},
		Project:              "tsconfig.json",
		Selections:           []tsfacts.Selection{selectionAt(source, "callable", 1)},
	})
	assert.NilError(t, tsfacts.ValidateResult(negotiated))
	deep := typeByID(t, negotiated, negotiated.Facts[0].ActualType)
	assert.Equal(t, len(deep.Properties), 1)
	assert.Equal(t, len(deep.CallSignatures), 1)
	assert.Assert(t, deep.Symbol != "")
	assert.Assert(t, negotiated.Facts[0].Complete)
}

func TestAnalyzePropagatesObjectGraphBudgetTruncation(t *testing.T) {
	t.Parallel()
	const source = `
interface KernelLeaf { value: string }
interface KernelChild { leaf: KernelLeaf }
interface KernelRoot { child: KernelChild }
declare const root: KernelRoot;
root;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		RequiredCapabilities: []string{
			tsfacts.CapabilityCoreCompositeTypes,
			tsfacts.CapabilityGraphReferences,
		},
		Budgets:    tsfacts.BudgetLimits{MaxTypeNodes: 64, MaxTypeDepth: 1},
		Project:    "tsconfig.json",
		Selections: []tsfacts.Selection{selectionAt(source, "root", 1)},
	})
	assert.NilError(t, tsfacts.ValidateResult(result))

	root := typeByID(t, result, result.Facts[0].ActualType)
	assert.Equal(t, root.State, tsfacts.EntityStateTruncated)
	assert.Assert(t, slices.ContainsFunc(root.Issues, func(issue tsfacts.GraphIssue) bool {
		return issue.Code == tsfacts.GraphIssueReferencedIncompleteSymbol
	}))
	assert.Assert(t, result.Facts[0].Truncated)
	assert.Assert(t, !result.Facts[0].Complete)
	assert.Assert(t, result.Header.Budgets.Truncated)
}

func signatureByID(t *testing.T, result *tsfacts.Result, id tsfacts.SignatureID) tsfacts.SignatureRecord {
	t.Helper()
	for _, record := range result.Signatures {
		if record.ID == id {
			return record
		}
	}
	t.Fatalf("signature %q not found", id)
	return tsfacts.SignatureRecord{}
}

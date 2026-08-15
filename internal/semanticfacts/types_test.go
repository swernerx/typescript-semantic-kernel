package semanticfacts_test

import (
	"bytes"
	"slices"
	"testing"

	tsfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	transport "github.com/microsoft/typescript-go/internal/tsfacts"
	"gotest.tools/v3/assert"
)

func TestAnalyzeExportsIntrinsicLiteralAndUniqueSymbolTypes(t *testing.T) {
	t.Parallel()
	const source = `
declare const anyValue: any;
declare const unknownValue: unknown;
declare const neverValue: never;
declare const undefinedValue: undefined;
declare const nullValue: null;
declare const voidValue: void;
declare const stringValue: string;
declare const numberValue: number;
declare const bigintValue: bigint;
declare const booleanValue: boolean;
declare const symbolValue: symbol;
declare const objectValue: object;
const stringLiteral = "hello" as const;
const booleanLiteral = true as const;
declare const numberLiteral: 42;
declare const bigintLiteral: 42n;
enum Choice { Yes = "yes" }
declare const enumLiteral: Choice.Yes;
declare const uniqueValue: unique symbol;
anyValue; unknownValue; neverValue; undefinedValue; nullValue; voidValue;
stringValue; numberValue; bigintValue; booleanValue; symbolValue; objectValue;
stringLiteral; booleanLiteral; numberLiteral; bigintLiteral; enumLiteral; uniqueValue;
`
	names := []string{
		"anyValue", "unknownValue", "neverValue", "undefinedValue", "nullValue", "voidValue",
		"stringValue", "numberValue", "bigintValue", "booleanValue", "symbolValue", "objectValue",
		"stringLiteral", "booleanLiteral", "numberLiteral", "bigintLiteral", "enumLiteral", "uniqueValue",
	}
	expectedKinds := []string{
		"any", "unknown", "never", "undefined", "null", "void",
		"string", "number", "bigint", "boolean", "symbol", "non_primitive",
		"literal", "literal", "literal", "literal", "literal", "unique_symbol",
	}
	selections := make([]tsfacts.Selection, 0, len(names))
	for _, name := range names {
		selections = append(selections, selectionAt(source, name, 1))
	}
	request := tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityCoreCompositeTypes},
		Project:              "tsconfig.json",
		Selections:           selections,
	}
	result := analyzeFixture(t, source, request)
	assert.NilError(t, tsfacts.ValidateResult(result))

	assert.Equal(t, len(result.Facts), len(expectedKinds))
	for index, expected := range expectedKinds {
		record := typeByID(t, result, result.Facts[index].ActualType)
		assert.Equal(t, record.TypeKind, expected, names[index])
		assert.Assert(t, len(record.Flags) != 0)
	}
	literals := []struct {
		index int
		kind  string
		value string
	}{
		{index: 12, kind: "string", value: "hello"},
		{index: 13, kind: "boolean", value: "true"},
		{index: 14, kind: "number", value: "42"},
		{index: 15, kind: "bigint", value: "42"},
		{index: 16, kind: "enum", value: "yes"},
	}
	for _, expected := range literals {
		literal := typeByID(t, result, result.Facts[expected.index].ActualType).Literal
		assert.Equal(t, literal.Kind, expected.kind)
		assert.Equal(t, literal.Value, expected.value)
	}
}

func TestAnalyzeExportsArraysTuplesAndGenericReferences(t *testing.T) {
	t.Parallel()
	const source = `
interface Array<T> { readonly length: number }
interface ReadonlyArray<T> { readonly length: number }
interface Box<T> { value: T }
declare const mutableValues: string[];
declare const readonlyValues: readonly number[];
declare const tupleValue: readonly [name: string, count?: number, ...flags: boolean[]];
declare const firstBox: Box<string>;
declare const secondBox: Box<string>;
mutableValues; readonlyValues; tupleValue; firstBox; secondBox;
`
	request := tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityCoreCompositeTypes},
		Project:              "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "mutableValues", 1),
			selectionAt(source, "readonlyValues", 1),
			selectionAt(source, "tupleValue", 1),
			selectionAt(source, "firstBox", 1),
			selectionAt(source, "secondBox", 1),
		},
	}
	result := analyzeFixture(t, source, request)
	assert.NilError(t, tsfacts.ValidateResult(result))

	mutableArray := typeByID(t, result, result.Facts[0].ActualType)
	assert.Equal(t, mutableArray.TypeKind, "array")
	assert.Assert(t, mutableArray.Target != "")
	assert.Equal(t, len(mutableArray.TypeArguments), 1)
	assert.Assert(t, mutableArray.Array != nil)
	assert.Assert(t, !mutableArray.Array.Readonly)
	assert.Equal(t, typeByID(t, result, mutableArray.TypeArguments[0]).TypeKind, "string")

	readonlyArray := typeByID(t, result, result.Facts[1].ActualType)
	assert.Equal(t, readonlyArray.TypeKind, "array")
	assert.Assert(t, readonlyArray.Array != nil)
	assert.Assert(t, readonlyArray.Array.Readonly)
	assert.Equal(t, typeByID(t, result, readonlyArray.TypeArguments[0]).TypeKind, "number")

	tuple := typeByID(t, result, result.Facts[2].ActualType)
	assert.Equal(t, tuple.TypeKind, "tuple")
	assert.Assert(t, tuple.Target != "")
	assert.Equal(t, len(tuple.TypeArguments), 3)
	assert.Assert(t, tuple.Tuple != nil)
	assert.Assert(t, tuple.Tuple.Readonly)
	assert.DeepEqual(t, tuple.Tuple.Elements, []tsfacts.TupleElementDetails{
		{Kind: "required", Label: "name"},
		{Kind: "optional", Label: "count"},
		{Kind: "rest", Label: "flags"},
	})

	firstReference := typeByID(t, result, result.Facts[3].ActualType)
	assert.Equal(t, firstReference.TypeKind, "reference")
	assert.Assert(t, firstReference.Target != "")
	assert.Equal(t, len(firstReference.TypeArguments), 1)
	assert.Equal(t, typeByID(t, result, firstReference.TypeArguments[0]).TypeKind, "string")
	assert.Equal(t, result.Facts[3].ActualType, result.Facts[4].ActualType)
	target := typeByID(t, result, firstReference.Target)
	assert.Equal(t, target.TypeKind, "reference")
	assert.Equal(t, target.Target, target.ID)
	assert.Equal(t, len(target.TypeArguments), 1)
	assert.Equal(t, typeByID(t, result, target.TypeArguments[0]).TypeKind, "type_parameter")
}

func TestAnalyzeGatesNewTypeVariantsOnCoreCompositeCapability(t *testing.T) {
	t.Parallel()
	const source = `declare const values: string[]; values;`
	request := tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "values", 1)},
	}
	result := analyzeFixture(t, source, request)

	record := typeByID(t, result, result.Facts[0].ActualType)
	assert.Equal(t, record.TypeKind, "object")
	assert.Equal(t, record.State, tsfacts.EntityStateTruncated)
	assert.Assert(t, record.Array == nil)
}

func TestAnalyzeExportsTypeParameterThisConstraintAndDefaultEdges(t *testing.T) {
	t.Parallel()
	const source = `
class Box<T extends string = "fallback"> {
    read(this: this, value: T): this { return this; }
}
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityCoreCompositeTypes},
		Project:              "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "T", 0),
			selectionAt(source, "this", 1),
		},
	})
	assert.NilError(t, tsfacts.ValidateResult(result))

	parameter := typeByID(t, result, result.Facts[0].ActualType)
	assert.Equal(t, parameter.TypeKind, "type_parameter")
	assert.Assert(t, parameter.Constraint != "")
	assert.Equal(t, typeByID(t, result, parameter.Constraint).TypeKind, "string")
	assert.Assert(t, parameter.Default != "")
	defaultType := typeByID(t, result, parameter.Default)
	assert.Equal(t, defaultType.TypeKind, "literal")
	assert.Equal(t, defaultType.Literal.Value, "fallback")

	thisType := typeByID(t, result, result.Facts[1].ActualType)
	assert.Equal(t, thisType.TypeKind, "this")
	assert.Assert(t, thisType.Constraint != "")
}

func TestAnalyzePreservesCheckerNormalizedCompositeMemberOrder(t *testing.T) {
	t.Parallel()
	const source = `
interface Alpha { alpha: string }
interface Beta { beta: number }
declare const unionLeft: Alpha | Beta;
declare const unionRight: Beta | Alpha;
declare const intersectionLeft: Alpha & Beta;
declare const intersectionRight: Beta & Alpha;
unionLeft; unionRight; intersectionLeft; intersectionRight;
`
	request := tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityCoreCompositeTypes},
		Project:              "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "unionLeft", 1),
			selectionAt(source, "unionRight", 1),
			selectionAt(source, "intersectionLeft", 1),
			selectionAt(source, "intersectionRight", 1),
		},
	}
	result := analyzeFixture(t, source, request)
	assert.NilError(t, tsfacts.ValidateResult(result))

	unionLeft := typeByID(t, result, result.Facts[0].ActualType)
	unionRight := typeByID(t, result, result.Facts[1].ActualType)
	assert.Equal(t, unionLeft.TypeKind, "union")
	assert.DeepEqual(t, typeDisplays(t, result, unionLeft.Members), typeDisplays(t, result, unionRight.Members))

	intersectionLeft := typeByID(t, result, result.Facts[2].ActualType)
	intersectionRight := typeByID(t, result, result.Facts[3].ActualType)
	assert.Equal(t, intersectionLeft.TypeKind, "intersection")
	assert.DeepEqual(t, typeDisplays(t, result, intersectionLeft.Members), typeDisplays(t, result, intersectionRight.Members))
	assert.Assert(t, slices.IsSorted(result.Header.Capabilities))

	repeated := analyzeFixture(t, source, request)
	var firstOutput bytes.Buffer
	var repeatedOutput bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&firstOutput, result))
	assert.NilError(t, transport.WriteJSONLines(&repeatedOutput, repeated))
	assert.Equal(t, firstOutput.String(), repeatedOutput.String())
}

func typeDisplays(t *testing.T, result *tsfacts.Result, ids []tsfacts.TypeID) []string {
	t.Helper()
	displays := make([]string, 0, len(ids))
	for _, id := range ids {
		displays = append(displays, typeByID(t, result, id).Display)
	}
	return displays
}

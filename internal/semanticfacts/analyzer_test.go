package semanticfacts_test

import (
	"bytes"
	"slices"
	"strings"
	"testing"

	"github.com/microsoft/typescript-go/internal/bundled"
	tsfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	transport "github.com/microsoft/typescript-go/internal/tsfacts"
	"github.com/microsoft/typescript-go/internal/vfs/vfstest"
	"gotest.tools/v3/assert"
)

func TestAnalyzeSelectedSemanticViews(t *testing.T) {
	t.Parallel()
	const source = `
declare const condition: boolean;
const greeting: string = "hello";
let value: string | number = condition ? greeting : 1;
if (typeof value === "string") {
    value;
}
const callable = (input: string) => input;
enum Choice { Yes = "yes" }
const choice = Choice.Yes;
`
	request := tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Files:         []string{"src/example.ts"},
		Selections: []tsfacts.Selection{
			selectionAt(source, "\"hello\"", 0),
			selectionAt(source, "value", 0),
			selectionAt(source, "value", 2),
			selectionAt(source, "callable", 0),
			selectionAt(source, "Yes", 1),
		},
	}

	result := analyzeFixture(t, source, request)
	assert.Equal(t, result.Header.OffsetEncoding, tsfacts.OffsetEncoding)
	assert.Equal(t, result.Header.DiagnosticCount, 0)
	assert.Equal(t, len(result.Facts), 5)

	literal := typeByID(t, result, result.Facts[0].TypeAtLocation)
	assert.Equal(t, result.Facts[0].ActualType, result.Facts[0].TypeAtLocation)
	assert.Equal(t, result.Facts[0].TypeViewStates.Actual, tsfacts.TypeViewAvailable)
	assert.Equal(t, literal.TypeKind, "literal")
	assert.Equal(t, literal.Literal.Kind, "string")
	assert.Equal(t, literal.Literal.Value, "hello")
	assert.Assert(t, result.Facts[0].ContextualType != "")
	assert.Equal(t, result.Facts[0].TypeViewStates.Contextual, tsfacts.TypeViewAvailable)
	assert.Equal(t, typeByID(t, result, result.Facts[0].ContextualType).TypeKind, "string")

	union := typeByID(t, result, result.Facts[1].TypeAtLocation)
	assert.Equal(t, union.TypeKind, "union")
	assert.Equal(t, len(union.Members), 2)
	assert.Assert(t, result.Facts[1].Complete)

	narrowed := typeByID(t, result, result.Facts[2].TypeAtLocation)
	assert.Equal(t, narrowed.TypeKind, "string")
	assert.Assert(t, result.Facts[2].ApparentType != "")
	assert.Equal(t, typeByID(t, result, result.Facts[2].ApparentType).TypeKind, "object")
	assert.Assert(t, result.Facts[2].Truncated)
	assert.Assert(t, !result.Facts[2].Complete)

	callable := typeByID(t, result, result.Facts[3].TypeAtLocation)
	assert.Equal(t, callable.TypeKind, "callable")
	assert.Assert(t, result.Facts[3].Truncated)
	assert.Assert(t, !result.Facts[3].Complete)

	enum := typeByID(t, result, result.Facts[4].TypeAtLocation)
	assert.Equal(t, enum.TypeKind, "literal")
	assert.Equal(t, enum.Literal.Kind, "enum")
}

func TestAnalyzeDistinguishesAnnotatedInferredAndNarrowedTypes(t *testing.T) {
	t.Parallel()
	const source = `
declare const condition: boolean;
let annotated: string | number = condition ? "text" : 1;
annotated;
let inferred = condition ? "text" : 1;
inferred;
if (typeof annotated === "string") {
    annotated;
}
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "annotated", 1),
			selectionAt(source, "inferred", 1),
			selectionAt(source, "annotated", 3),
		},
	})

	annotated := result.Facts[0]
	assert.Assert(t, annotated.AnnotationType != "")
	assert.Equal(t, annotated.InferredType, tsfacts.TypeID(""))
	assert.Equal(t, annotated.NarrowedType, tsfacts.TypeID(""))
	assert.Equal(t, annotated.TypeAtLocation, annotated.AnnotationType)
	assert.Equal(t, typeByID(t, result, annotated.AnnotationType).TypeKind, "union")

	inferred := result.Facts[1]
	assert.Equal(t, inferred.AnnotationType, tsfacts.TypeID(""))
	assert.Assert(t, inferred.InferredType != "")
	assert.Equal(t, inferred.NarrowedType, tsfacts.TypeID(""))
	assert.Equal(t, inferred.TypeAtLocation, inferred.InferredType)
	assert.Equal(t, typeByID(t, result, inferred.InferredType).TypeKind, "union")

	narrowed := result.Facts[2]
	assert.Assert(t, narrowed.AnnotationType != "")
	assert.Equal(t, narrowed.InferredType, tsfacts.TypeID(""))
	assert.Assert(t, narrowed.NarrowedType != "")
	assert.Equal(t, narrowed.TypeAtLocation, narrowed.NarrowedType)
	assert.Equal(t, typeByID(t, result, narrowed.AnnotationType).TypeKind, "union")
	assert.Equal(t, typeByID(t, result, narrowed.NarrowedType).TypeKind, "string")
	assert.Assert(t, narrowed.DeclaredType != "")
	assert.Equal(t, narrowed.TypeViewStates.Declared, tsfacts.TypeViewAvailable)
	assert.Equal(t, narrowed.DeclaredType, narrowed.AnnotationType)
}

func TestAnalyzeReportsOptionalTypeViewAvailability(t *testing.T) {
	t.Parallel()
	const source = `
function read<T extends { value: string }>(input: T) {
    input;
}
interface Message { text: string }
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "input", 1),
			selectionAt(source, "Message", 0),
		},
	})

	typeParameter := result.Facts[0]
	assert.Equal(t, typeParameter.ActualType, typeParameter.TypeAtLocation)
	assert.Equal(t, typeParameter.TypeViewStates.Contextual, tsfacts.TypeViewUnavailable)
	assert.Assert(t, typeParameter.ApparentType != "")
	assert.Equal(t, typeParameter.TypeViewStates.Apparent, tsfacts.TypeViewAvailable)
	assert.Equal(t, typeParameter.TypeViewStates.Declared, tsfacts.TypeViewSameAsActual)

	typeDeclaration := result.Facts[1]
	assert.Equal(t, typeDeclaration.TypeViewStates.Contextual, tsfacts.TypeViewInapplicable)
	assert.Equal(t, typeDeclaration.TypeViewStates.Widened, tsfacts.TypeViewSameAsActual)
	assert.Equal(t, typeDeclaration.TypeViewStates.Apparent, tsfacts.TypeViewSameAsActual)
	assert.Equal(t, typeDeclaration.TypeViewStates.Declared, tsfacts.TypeViewSameAsActual)
}

func TestAnalyzeDoesNotMisclassifyContainingAnnotations(t *testing.T) {
	t.Parallel()
	const source = `
const { text }: { text: string } = { text: "value" };
text;
function parse(): string { return text; }
parse;
class Box {}
declare let box: Box;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "text", 3),
			selectionAt(source, "parse", 1),
			selectionAt(source, "Box", 1),
		},
	})

	destructured := result.Facts[0]
	assert.Equal(t, destructured.AnnotationType, tsfacts.TypeID(""))
	assert.Assert(t, destructured.InferredType != "")
	assert.Equal(t, typeByID(t, result, destructured.InferredType).TypeKind, "string")

	function := result.Facts[1]
	assert.Equal(t, function.AnnotationType, tsfacts.TypeID(""))
	assert.Assert(t, function.InferredType != "")
	assert.Equal(t, typeByID(t, result, function.InferredType).TypeKind, "callable")

	typePosition := result.Facts[2]
	assert.Equal(t, typePosition.AnnotationType, tsfacts.TypeID(""))
	assert.Equal(t, typePosition.InferredType, tsfacts.TypeID(""))
	assert.Equal(t, typePosition.NarrowedType, tsfacts.TypeID(""))
}

func TestAnalyzeUsesDeclaredTypeAsNarrowingBaseline(t *testing.T) {
	t.Parallel()
	const source = `
function consume(value?: string) {
    value;
    if (value) {
        value;
    }
}
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "value", 1),
			selectionAt(source, "value", 3),
		},
	})

	declared := result.Facts[0]
	assert.Assert(t, declared.AnnotationType != "")
	assert.Equal(t, typeByID(t, result, declared.AnnotationType).TypeKind, "string")
	assert.Equal(t, typeByID(t, result, declared.TypeAtLocation).TypeKind, "union")
	assert.Equal(t, declared.NarrowedType, tsfacts.TypeID(""))

	narrowed := result.Facts[1]
	assert.Equal(t, narrowed.AnnotationType, declared.AnnotationType)
	assert.Assert(t, narrowed.NarrowedType != "")
	assert.Equal(t, narrowed.TypeAtLocation, narrowed.NarrowedType)
	assert.Equal(t, typeByID(t, result, narrowed.NarrowedType).TypeKind, "string")
}

func TestAnalyzeDoesNotMisclassifyGenericInstantiationAsNarrowing(t *testing.T) {
	t.Parallel()
	const source = `
interface Box<T> { value: T }
declare const box: Box<string>;
box.value;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "value", 1)},
	})

	fact := result.Facts[0]
	assert.Equal(t, typeByID(t, result, fact.TypeAtLocation).TypeKind, "string")
	assert.Equal(t, fact.NarrowedType, tsfacts.TypeID(""))
}

func TestAnalyzeMarksPropertyOfNarrowedReceiver(t *testing.T) {
	t.Parallel()
	const source = `
type Entity =
    | { kind: "text"; value: string }
    | { kind: "count"; value: number };
declare const entity: Entity;
if (entity.kind === "text") {
    entity.value;
}
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "value", 2)},
	})

	fact := result.Facts[0]
	assert.Equal(t, typeByID(t, result, fact.TypeAtLocation).TypeKind, "string")
	assert.Assert(t, fact.NarrowedType != "")
	assert.Equal(t, fact.TypeAtLocation, fact.NarrowedType)
}

func TestAnalyzeHandlesQualifiedTypeQuery(t *testing.T) {
	t.Parallel()
	const source = `
namespace Values { export const value = "text" as const; }
type Value = typeof Values.value;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "value", 1)},
	})

	fact := result.Facts[0]
	assert.Equal(t, typeByID(t, result, fact.TypeAtLocation).TypeKind, "literal")
	assert.Equal(t, fact.NarrowedType, tsfacts.TypeID(""))
}

func TestAnalyzeEmitsSymbolAndDeclarationProvenance(t *testing.T) {
	t.Parallel()
	const source = `
interface Message { text: string }
declare const message: Message;
message.text;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "message", 1)},
	})

	fact := result.Facts[0]
	assert.Assert(t, fact.Symbol != "")
	assert.Equal(t, len(fact.Declarations), 1)
	symbol := symbolByID(t, result, fact.Symbol)
	assert.Equal(t, symbol.Name, "message")
	assert.Assert(t, slices.Contains(symbol.Roles, "variable"))
	assert.DeepEqual(t, symbol.Declarations, fact.Declarations)
	assert.Assert(t, symbol.Complete)
	assert.Assert(t, !symbol.Truncated)

	declaration := declarationByID(t, result, fact.Declarations[0])
	assert.Equal(t, declaration.File, "src/example.ts")
	assert.Equal(t, declaration.SyntaxKind, "KindVariableDeclaration")
	declarationSelection := selectionAt(source, "message", 0)
	assert.DeepEqual(t, declaration.Span, tsfacts.Span{Start: declarationSelection.Start, End: declarationSelection.End})
}

func TestAnalyzePreservesAliasAndTargetSymbols(t *testing.T) {
	t.Parallel()
	const source = `import { greeting as localGreeting } from "./values"; localGreeting;`
	const values = `export const greeting = "hello";`
	fs := vfstest.FromMap(map[string]string{
		"/project/tsconfig.json":  `{"compilerOptions":{"strict":true,"noEmit":true,"module":"preserve"},"files":["src/example.ts","src/values.ts"]}`,
		"/project/src/example.ts": source,
		"/project/src/values.ts":  values,
	}, true)
	result, err := tsfacts.Analyze(t.Context(), tsfacts.AnalyzerOptions{
		CurrentDirectory:   "/project",
		FS:                 bundled.WrapFS(fs),
		DefaultLibraryPath: bundled.LibPath(),
	}, tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityGraphReferences},
		Project:              "tsconfig.json",
		Selections:           []tsfacts.Selection{selectionAt(source, "localGreeting", 1)},
	})
	assert.NilError(t, err)

	alias := symbolByID(t, result, result.Facts[0].Symbol)
	assert.Equal(t, alias.Name, "localGreeting")
	assert.Assert(t, slices.Contains(alias.Roles, "alias"))
	assert.Assert(t, alias.AliasedSymbol != "")
	assert.Equal(t, declarationByID(t, result, alias.Declarations[0]).File, "src/example.ts")

	target := symbolByID(t, result, alias.AliasedSymbol)
	assert.Equal(t, target.Name, "greeting")
	assert.Assert(t, slices.Contains(target.Roles, "variable"))
	assert.Equal(t, declarationByID(t, result, target.Declarations[0]).File, "src/values.ts")
	assert.Assert(t, target.Type != "")
	assert.Assert(t, alias.Complete)
	assert.Assert(t, target.Complete)

	selectedFile := fileByID(t, result, "src/example.ts")
	assert.Assert(t, selectedFile.Selected)
	assert.Assert(t, selectedFile.DiagnosticCount != nil)
	assert.Equal(t, *selectedFile.DiagnosticCount, 0)
	declarationFile := fileByID(t, result, "src/values.ts")
	assert.Assert(t, !declarationFile.Selected)
	assert.Assert(t, declarationFile.DiagnosticCount == nil)
	var output bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&output, result))
	declarationFileLine := lineContaining(t, strings.Split(output.String(), "\n"), `"id":"src/values.ts"`)
	assert.Assert(t, strings.Contains(declarationFileLine, `"origin":"project"`))
	assert.Assert(t, !strings.Contains(declarationFileLine, `"selected"`))
	assert.Assert(t, !strings.Contains(declarationFileLine, `"diagnosticCount"`))
}

func TestAnalyzeUsesStableTypeScriptLibraryIdentity(t *testing.T) {
	t.Parallel()
	const source = `console;`
	fs := vfstest.FromMap(map[string]string{
		"/lib/lib.d.ts":           `interface Console {} declare const console: Console;`,
		"/project/tsconfig.json":  `{"compilerOptions":{"strict":true,"noEmit":true,"target":"es5"},"files":["src/example.ts"]}`,
		"/project/src/example.ts": source,
	}, true)
	result, err := tsfacts.Analyze(t.Context(), tsfacts.AnalyzerOptions{
		CurrentDirectory:   "/project",
		FS:                 fs,
		DefaultLibraryPath: "/lib",
	}, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "console", 0)},
	})
	assert.NilError(t, err)

	symbol := symbolByID(t, result, result.Facts[0].Symbol)
	assert.Equal(t, declarationByID(t, result, symbol.Declarations[0]).File, "typescript/lib/lib.d.ts")
	library := fileByID(t, result, "typescript/lib/lib.d.ts")
	assert.Equal(t, library.Origin, "typescript-lib")
	assert.Assert(t, !library.Selected)
	assert.Assert(t, library.DiagnosticCount == nil)
}

func TestAnalyzeMarksUnsupportedExternalDeclarationTruncated(t *testing.T) {
	t.Parallel()
	const source = `import { greeting } from "../../shared/values"; greeting;`
	const values = `export const greeting = "hello";`
	fs := vfstest.FromMap(map[string]string{
		"/project/tsconfig.json":  `{"compilerOptions":{"strict":true,"noEmit":true,"module":"preserve"},"files":["src/example.ts","../shared/values.ts"]}`,
		"/project/src/example.ts": source,
		"/shared/values.ts":       values,
	}, true)
	result, err := tsfacts.Analyze(t.Context(), tsfacts.AnalyzerOptions{
		CurrentDirectory:   "/project",
		FS:                 bundled.WrapFS(fs),
		DefaultLibraryPath: bundled.LibPath(),
	}, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "greeting", 1)},
	})
	assert.NilError(t, err)

	fact := result.Facts[0]
	alias := symbolByID(t, result, fact.Symbol)
	target := symbolByID(t, result, alias.AliasedSymbol)
	assert.Assert(t, alias.Truncated)
	assert.Assert(t, target.Truncated)
	assert.Equal(t, len(target.Declarations), 0)
	assert.Assert(t, fact.Truncated)
	for _, file := range result.Files {
		assert.Assert(t, !strings.Contains(file.ID, "/shared"))
	}
}

func TestAnalyzePreservesMergedDeclarationsInSourceOrder(t *testing.T) {
	t.Parallel()
	const source = `
interface Box { text: string }
interface Box { count: number }
declare const box: Box;
`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "Box", 2)},
	})

	symbol := symbolByID(t, result, result.Facts[0].Symbol)
	assert.Equal(t, symbol.Name, "Box")
	assert.Assert(t, slices.Contains(symbol.Roles, "interface"))
	assert.Equal(t, len(symbol.Declarations), 2)
	first := declarationByID(t, result, symbol.Declarations[0])
	second := declarationByID(t, result, symbol.Declarations[1])
	assert.Assert(t, first.Span.Start < second.Span.Start)
}

func TestAnalyzeCanonicalizesCaseInsensitiveFileIdentity(t *testing.T) {
	t.Parallel()
	const source = `const value = 1;`
	fs := vfstest.FromMap(map[string]string{
		"/project/tsconfig.json":  `{"compilerOptions":{"noEmit":true},"files":["src/example.ts"]}`,
		"/project/src/example.ts": source,
	}, false)
	selection := selectionAt(source, "value", 0)
	selection.File = "SRC/EXAMPLE.TS"
	result, err := tsfacts.Analyze(t.Context(), tsfacts.AnalyzerOptions{
		CurrentDirectory:   "/project",
		FS:                 bundled.WrapFS(fs),
		DefaultLibraryPath: bundled.LibPath(),
	}, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Files:         []string{"src/example.ts"},
		Selections:    []tsfacts.Selection{selection},
	})
	assert.NilError(t, err)
	assert.Equal(t, result.Files[0].ID, "src/example.ts")
	assert.Equal(t, result.Facts[0].File, "src/example.ts")
}

func TestAnalyzeUsesUTF8ByteOffsets(t *testing.T) {
	t.Parallel()
	const source = `const café = "🌍";`
	selection := selectionAt(source, "\"🌍\"", 0)
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selection},
	})

	fact := result.Facts[0]
	assert.DeepEqual(t, fact.Span, tsfacts.Span{Start: selection.Start, End: selection.End})
	assert.Equal(t, fact.Span.End-fact.Span.Start, len("\"🌍\""))
}

func TestAnalyzeMarksRecoveredFacts(t *testing.T) {
	t.Parallel()
	const source = `const value: string = 1; value;`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "value", 1)},
	})

	assert.Assert(t, result.Header.DiagnosticCount > 0)
	assert.Assert(t, result.Facts[0].Recovered)
	assert.Assert(t, !result.Facts[0].Complete)
}

func TestAnalyzeEnumeratesFileWideOccurrencesDeterministically(t *testing.T) {
	t.Parallel()
	const source = `const value: string = "hello"; value;`
	request := tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Files:         []string{"src/example.ts"},
	}
	first := analyzeFixture(t, source, request)
	second := analyzeFixture(t, source, request)

	assert.Assert(t, len(first.Facts) >= 4)
	for index, fact := range first.Facts {
		assert.Equal(t, fact.File, "src/example.ts")
		assert.Equal(t, fact.TypeViewStates.Actual, tsfacts.TypeViewAvailable)
		assert.Assert(t, fact.TypeViewStates.Contextual != "")
		assert.Assert(t, fact.TypeViewStates.Widened != "")
		assert.Assert(t, fact.TypeViewStates.Apparent != "")
		assert.Assert(t, fact.TypeViewStates.Declared != "")
		if index != 0 {
			assert.Assert(t, first.Facts[index-1].Span.Start < fact.Span.Start)
		}
	}
	for _, selection := range []tsfacts.Selection{
		selectionAt(source, "value", 0),
		selectionAt(source, "string", 0),
		selectionAt(source, `"hello"`, 0),
		selectionAt(source, "value", 1),
	} {
		assert.Assert(t, factAtStart(first, selection.Start) != nil)
	}
	literal := factAtStart(first, selectionAt(source, `"hello"`, 0).Start)
	assert.Equal(t, literal.TypeViewStates.Contextual, tsfacts.TypeViewAvailable)
	assert.Assert(t, literal.ContextualType != "")
	assert.Assert(t, literal.ContextualType != literal.ActualType)

	var firstOutput bytes.Buffer
	var secondOutput bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&firstOutput, first))
	assert.NilError(t, transport.WriteJSONLines(&secondOutput, second))
	assert.Equal(t, firstOutput.String(), secondOutput.String())
}

func TestAnalyzeOrdersFileWideOccurrencesByCanonicalFileID(t *testing.T) {
	t.Parallel()
	fs := vfstest.FromMap(map[string]string{
		"/project/tsconfig.json": `{"compilerOptions":{"strict":true,"noEmit":true},"files":["src/a.ts","src/z.ts"]}`,
		"/project/src/a.ts":      `const alpha = 1;`,
		"/project/src/z.ts":      `const zeta = 2;`,
	}, true)
	result, err := tsfacts.Analyze(t.Context(), tsfacts.AnalyzerOptions{
		CurrentDirectory:   "/project",
		FS:                 bundled.WrapFS(fs),
		DefaultLibraryPath: bundled.LibPath(),
	}, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Files:         []string{"src/z.ts", "src/a.ts"},
	})
	assert.NilError(t, err)
	assert.Assert(t, len(result.Facts) >= 4)
	seenZ := false
	for _, fact := range result.Facts {
		if fact.File == "src/z.ts" {
			seenZ = true
		} else {
			assert.Equal(t, fact.File, "src/a.ts")
			assert.Assert(t, !seenZ)
		}
	}
}

func TestAnalyzeRequiresAFileOrSelectionScope(t *testing.T) {
	t.Parallel()
	const source = `const value = 1;`
	_, err := tsfacts.Analyze(t.Context(), fixtureOptions(source), tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
	})
	assert.ErrorContains(t, err, "at least one file or selection is required")
}

func TestAnalyzeNegotiatesCapabilitiesAndReportsDefaultBudgets(t *testing.T) {
	t.Parallel()
	const source = `const value = 1; value;`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityExplicitStates, tsfacts.CapabilityTypeBudgets},
		Project:              "tsconfig.json",
		Selections:           []tsfacts.Selection{selectionAt(source, "value", 1)},
	})

	assert.DeepEqual(t, result.Header.Capabilities, tsfacts.SupportedCapabilities())
	assert.DeepEqual(t, result.Header.Budgets.Limits, tsfacts.BudgetLimits{
		MaxTypeNodes: tsfacts.DefaultMaxTypeNodes,
		MaxTypeDepth: tsfacts.DefaultMaxTypeDepth,
	})
	assert.Equal(t, result.Header.Budgets.TypeNodesUsed, len(result.Types))
	assert.Assert(t, !result.Header.Budgets.Truncated)
}

func TestAnalyzeRejectsUnsupportedRequiredCapability(t *testing.T) {
	t.Parallel()
	const source = `const value = 1; value;`
	_, err := tsfacts.Analyze(t.Context(), fixtureOptions(source), tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{"future.semantic-magic"},
		Project:              "tsconfig.json",
		Selections:           []tsfacts.Selection{selectionAt(source, "value", 1)},
	})

	assert.ErrorContains(t, err, `unsupported required capability "future.semantic-magic"`)
}

func TestAnalyzeRejectsInvalidBudgets(t *testing.T) {
	t.Parallel()
	const source = `const value = 1; value;`
	_, err := tsfacts.Analyze(t.Context(), fixtureOptions(source), tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Budgets:       tsfacts.BudgetLimits{MaxTypeNodes: -1},
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "value", 1)},
	})

	assert.ErrorContains(t, err, "budgets.maxTypeNodes must be positive")
}

func TestAnalyzeAppliesTypeNodeBudgetDeterministically(t *testing.T) {
	t.Parallel()
	const source = `declare const condition: boolean; const value: string | number = condition ? "text" : 1; value;`
	request := tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Budgets:       tsfacts.BudgetLimits{MaxTypeNodes: 1, MaxTypeDepth: 32},
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "value", 1)},
	}
	first := analyzeFixture(t, source, request)
	second := analyzeFixture(t, source, request)

	assert.DeepEqual(t, first.Header.Budgets, tsfacts.BudgetReport{
		Limits:               request.Budgets,
		TypeNodesUsed:        1,
		MaxTypeDepthObserved: 1,
		Truncated:            true,
	})
	assert.Equal(t, len(first.Types), 2)
	root := typeByID(t, first, first.Facts[0].ActualType)
	assert.Equal(t, root.State, tsfacts.EntityStateTruncated)
	assert.DeepEqual(t, root.Issues, []tsfacts.GraphIssue{{Code: tsfacts.GraphIssueReferencedIncompleteType}})
	limit := typeByID(t, first, root.Members[0])
	assert.Equal(t, limit.TypeKind, "truncated")
	assert.Equal(t, limit.State, tsfacts.EntityStateTruncated)
	assert.DeepEqual(t, limit.Issues, []tsfacts.GraphIssue{{Code: tsfacts.GraphIssueMaxTypeNodes, Limit: 1}})
	assert.Assert(t, first.Facts[0].Truncated)
	assert.Assert(t, !first.Facts[0].Complete)

	var firstOutput bytes.Buffer
	var secondOutput bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&firstOutput, first))
	assert.NilError(t, transport.WriteJSONLines(&secondOutput, second))
	assert.Equal(t, firstOutput.String(), secondOutput.String())
}

func TestAnalyzeAppliesTypeDepthBudget(t *testing.T) {
	t.Parallel()
	const source = `function use<T extends U | boolean, U extends string | number>(value: T) { value; }`
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Budgets:       tsfacts.BudgetLimits{MaxTypeNodes: 32, MaxTypeDepth: 1},
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "value", 1)},
	})

	assert.Assert(t, result.Header.Budgets.Truncated)
	assert.Assert(t, result.Header.Budgets.MaxTypeDepthObserved > result.Header.Budgets.Limits.MaxTypeDepth)
	depthSentinelFound := false
	for _, record := range result.Types {
		if slices.ContainsFunc(record.Issues, func(issue tsfacts.GraphIssue) bool {
			return issue.Code == tsfacts.GraphIssueMaxTypeDepth && issue.Limit == 1
		}) {
			depthSentinelFound = true
			assert.Equal(t, record.State, tsfacts.EntityStateTruncated)
		}
	}
	assert.Assert(t, depthSentinelFound)
	assert.Assert(t, result.Facts[0].Truncated)
}

func TestWriteJSONLinesIsDeterministic(t *testing.T) {
	t.Parallel()
	const source = `const value = "stable" as const; value;`
	request := tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "value", 1)},
	}
	first := analyzeFixture(t, source, request)
	second := analyzeFixture(t, source, request)
	var firstOutput bytes.Buffer
	var secondOutput bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&firstOutput, first))
	assert.NilError(t, transport.WriteJSONLines(&secondOutput, second))
	assert.Equal(t, firstOutput.String(), secondOutput.String())
	assert.Assert(t, strings.HasSuffix(firstOutput.String(), "\n"))
	assert.Equal(t, strings.Count(firstOutput.String(), "\n"), 1+len(first.Files)+len(first.Types)+len(first.Declarations)+len(first.Symbols)+len(first.Facts))
	lines := strings.Split(strings.TrimSpace(firstOutput.String()), "\n")
	typeIndex := recordIndex(t, lines, "type")
	declarationIndex := recordIndex(t, lines, "declaration")
	symbolIndex := recordIndex(t, lines, "symbol")
	factIndex := recordIndex(t, lines, "fact")
	assert.Assert(t, typeIndex < declarationIndex)
	assert.Assert(t, declarationIndex < symbolIndex)
	assert.Assert(t, symbolIndex < factIndex)
}

func TestAnalyzeRejectsSelectionAcrossTokens(t *testing.T) {
	t.Parallel()
	const source = `const value = 1;`
	selection := selectionAt(source, "value = 1", 0)
	_, err := tsfacts.Analyze(t.Context(), fixtureOptions(source), tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selection},
	})
	assert.ErrorContains(t, err, "must fit inside one token")
}

func analyzeFixture(t *testing.T, source string, request tsfacts.Request) *tsfacts.Result {
	t.Helper()
	result, err := tsfacts.Analyze(t.Context(), fixtureOptions(source), request)
	assert.NilError(t, err)
	return result
}

func fixtureOptions(source string) tsfacts.AnalyzerOptions {
	fs := vfstest.FromMap(map[string]string{
		"/project/tsconfig.json":  `{"compilerOptions":{"strict":true,"noEmit":true},"files":["src/example.ts"]}`,
		"/project/src/example.ts": source,
	}, true)
	return tsfacts.AnalyzerOptions{
		CurrentDirectory:   "/project",
		FS:                 bundled.WrapFS(fs),
		DefaultLibraryPath: bundled.LibPath(),
	}
}

func selectionAt(source string, text string, occurrence int) tsfacts.Selection {
	offset := 0
	for index := 0; index <= occurrence; index++ {
		match := strings.Index(source[offset:], text)
		if match == -1 {
			panic("selection text not found: " + text)
		}
		start := offset + match
		if index == occurrence {
			return tsfacts.Selection{File: "src/example.ts", Start: start, End: start + len(text)}
		}
		offset = start + len(text)
	}
	panic("unreachable")
}

func typeByID(t *testing.T, result *tsfacts.Result, id tsfacts.TypeID) tsfacts.TypeRecord {
	t.Helper()
	for _, record := range result.Types {
		if record.ID == id {
			return record
		}
	}
	t.Fatalf("type %q not found", id)
	return tsfacts.TypeRecord{}
}

func factAtStart(result *tsfacts.Result, start int) *tsfacts.FactRecord {
	for index := range result.Facts {
		if result.Facts[index].Span.Start == start {
			return &result.Facts[index]
		}
	}
	return nil
}

func symbolByID(t *testing.T, result *tsfacts.Result, id tsfacts.SymbolID) tsfacts.SymbolRecord {
	t.Helper()
	for _, record := range result.Symbols {
		if record.ID == id {
			return record
		}
	}
	t.Fatalf("symbol %q not found", id)
	return tsfacts.SymbolRecord{}
}

func declarationByID(t *testing.T, result *tsfacts.Result, id tsfacts.DeclarationID) tsfacts.DeclarationRecord {
	t.Helper()
	for _, record := range result.Declarations {
		if record.ID == id {
			return record
		}
	}
	t.Fatalf("declaration %q not found", id)
	return tsfacts.DeclarationRecord{}
}

func fileByID(t *testing.T, result *tsfacts.Result, id string) tsfacts.FileRecord {
	t.Helper()
	for _, record := range result.Files {
		if record.ID == id {
			return record
		}
	}
	t.Fatalf("file %q not found", id)
	return tsfacts.FileRecord{}
}

func recordIndex(t *testing.T, lines []string, record string) int {
	t.Helper()
	needle := `"record":"` + record + `"`
	for index, line := range lines {
		if strings.Contains(line, needle) {
			return index
		}
	}
	t.Fatalf("record %q not found", record)
	return -1
}

func lineContaining(t *testing.T, lines []string, needle string) string {
	t.Helper()
	for _, line := range lines {
		if strings.Contains(line, needle) {
			return line
		}
	}
	t.Fatalf("line containing %q not found", needle)
	return ""
}

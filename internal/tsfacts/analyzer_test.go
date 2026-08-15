package tsfacts_test

import (
	"bytes"
	"slices"
	"strings"
	"testing"

	"github.com/microsoft/typescript-go/internal/bundled"
	"github.com/microsoft/typescript-go/internal/tsfacts"
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
	assert.Equal(t, literal.TypeKind, "literal")
	assert.Equal(t, literal.Literal.Kind, "string")
	assert.Equal(t, literal.Literal.Value, "hello")
	assert.Assert(t, result.Facts[0].ContextualType != "")
	assert.Equal(t, typeByID(t, result, result.Facts[0].ContextualType).TypeKind, "string")

	union := typeByID(t, result, result.Facts[1].TypeAtLocation)
	assert.Equal(t, union.TypeKind, "union")
	assert.Equal(t, len(union.Members), 2)
	assert.Assert(t, result.Facts[1].Complete)

	narrowed := typeByID(t, result, result.Facts[2].TypeAtLocation)
	assert.Equal(t, narrowed.TypeKind, "string")
	assert.Assert(t, result.Facts[2].Complete)

	callable := typeByID(t, result, result.Facts[3].TypeAtLocation)
	assert.Equal(t, callable.TypeKind, "callable")
	assert.Assert(t, result.Facts[3].Truncated)
	assert.Assert(t, !result.Facts[3].Complete)

	enum := typeByID(t, result, result.Facts[4].TypeAtLocation)
	assert.Equal(t, enum.TypeKind, "literal")
	assert.Equal(t, enum.Literal.Kind, "enum")
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
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selectionAt(source, "localGreeting", 1)},
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
	assert.NilError(t, tsfacts.WriteJSONLines(&output, result))
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
	assert.NilError(t, tsfacts.WriteJSONLines(&firstOutput, first))
	assert.NilError(t, tsfacts.WriteJSONLines(&secondOutput, second))
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

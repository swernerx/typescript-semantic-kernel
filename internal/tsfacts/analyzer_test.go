package tsfacts_test

import (
	"bytes"
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
const greeting: string = "hello";
let value: string | number = Math.random() ? greeting : 1;
if (typeof value === "string") {
    value;
}
const callable = (input: string) => input.length;
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
	assert.Equal(t, strings.Count(firstOutput.String(), "\n"), 1+len(first.Files)+len(first.Types)+len(first.Facts))
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

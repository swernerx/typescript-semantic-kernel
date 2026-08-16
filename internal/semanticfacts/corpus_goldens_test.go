//go:build !noembed

package semanticfacts_test

import (
	"bytes"
	"cmp"
	"flag"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"testing"

	tsfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	transport "github.com/microsoft/typescript-go/internal/tsfacts"
	"gotest.tools/v3/assert"
)

var normalizedGoldenCases = []string{
	"advanced-types",
	"occurrence-contexts",
	"vertical-slice",
}

var updateSemanticFactsGoldens = flag.Bool("update-semantic-facts-goldens", false, "rewrite normalized semantic-facts corpus goldens")

func TestSemanticFactsCorpusSnapshotsAreNormalizedAndStable(t *testing.T) {
	t.Parallel()

	for _, caseName := range normalizedGoldenCases {
		t.Run(caseName, func(t *testing.T) {
			t.Parallel()

			first, _ := analyzeCorpusFixture(t, caseName)
			second, _ := analyzeCorpusFixture(t, caseName)
			firstSnapshot := normalizedCorpusSnapshot(t, first)
			secondSnapshot := normalizedCorpusSnapshot(t, second)
			assert.Equal(t, string(firstSnapshot), string(secondSnapshot))
			roundTrip, readErr := transport.ReadJSONLines(bytes.NewReader(firstSnapshot))
			assert.NilError(t, readErr)
			assert.NilError(t, tsfacts.ValidateResult(roundTrip))

			goldenPath := filepath.Join("testdata", "conformance", "v0", caseName+".jsonl")
			if *updateSemanticFactsGoldens {
				assert.NilError(t, os.WriteFile(goldenPath, firstSnapshot, 0o644))
			}
			expected, err := os.ReadFile(goldenPath)
			assert.NilError(t, err)
			assert.Equal(t, string(firstSnapshot), string(expected))
		})
	}
}

func normalizedCorpusSnapshot(t *testing.T, result *tsfacts.Result) []byte {
	t.Helper()
	normalized := *result
	normalized.Header = result.Header
	normalized.Header.TypeScriptVersion = "<normalized>"
	normalized.Header.TypeScriptRevision = "<normalized>"
	normalized.Header.Capabilities = slices.Clone(result.Header.Capabilities)
	slices.Sort(normalized.Header.Capabilities)
	normalized.Files = slices.Clone(result.Files)
	normalized.Types = slices.Clone(result.Types)
	normalized.Declarations = slices.Clone(result.Declarations)
	normalized.Symbols = slices.Clone(result.Symbols)
	normalized.Signatures = slices.Clone(result.Signatures)
	normalized.Facts = slices.Clone(result.Facts)
	slices.SortFunc(normalized.Files, func(left, right tsfacts.FileRecord) int { return cmp.Compare(left.ID, right.ID) })
	slices.SortFunc(normalized.Types, func(left, right tsfacts.TypeRecord) int { return cmp.Compare(left.ID, right.ID) })
	slices.SortFunc(normalized.Declarations, func(left, right tsfacts.DeclarationRecord) int { return cmp.Compare(left.ID, right.ID) })
	slices.SortFunc(normalized.Symbols, func(left, right tsfacts.SymbolRecord) int { return cmp.Compare(left.ID, right.ID) })
	slices.SortFunc(normalized.Signatures, func(left, right tsfacts.SignatureRecord) int { return cmp.Compare(left.ID, right.ID) })
	slices.SortFunc(normalized.Facts, compareFacts)

	var output bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&output, &normalized))
	transientSymbol := regexp.MustCompile(`__@([^"@]+)@[0-9]+`)
	return transientSymbol.ReplaceAll(output.Bytes(), []byte(`__@$1@<id>`))
}

func compareFacts(left, right tsfacts.FactRecord) int {
	if order := cmp.Compare(left.File, right.File); order != 0 {
		return order
	}
	if order := cmp.Compare(left.Span.Start, right.Span.Start); order != 0 {
		return order
	}
	if order := cmp.Compare(left.Span.End, right.Span.End); order != 0 {
		return order
	}
	return cmp.Compare(left.SyntaxKind, right.SyntaxKind)
}

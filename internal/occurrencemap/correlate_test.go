package occurrencemap_test

import (
	"os"
	"path/filepath"
	"slices"
	"testing"

	"github.com/microsoft/typescript-go/internal/json"
	"github.com/microsoft/typescript-go/internal/occurrencemap"
	semanticfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	"gotest.tools/v3/assert"
)

type correlationFixture struct {
	Description string                     `json:"description"`
	Sources     map[string]string          `json:"sources"`
	Facts       []semanticfacts.FactRecord `json:"facts"`
	Nodes       []occurrencemap.Node       `json:"nodes"`
	Expected    occurrencemap.Report       `json:"expected"`
}

func TestCorrelationFixtures(t *testing.T) {
	t.Parallel()

	paths, err := filepath.Glob(filepath.Join("testdata", "v1", "*.json"))
	assert.NilError(t, err)
	assert.DeepEqual(t, fixtureNames(paths), []string{"declarations", "expressions", "flow-sensitive", "jsx"})

	for _, path := range paths {
		name := filepath.Base(path[:len(path)-len(filepath.Ext(path))])
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			fixture := readFixture(t, path)
			assert.Assert(t, fixture.Description != "")
			validateFixtureSpans(t, fixture)
			actual, correlateErr := occurrencemap.Correlate(fixture.Facts, fixture.Nodes)
			assert.NilError(t, correlateErr)
			assert.DeepEqual(t, actual, fixture.Expected)

			reversedNodes := slices.Clone(fixture.Nodes)
			slices.Reverse(reversedNodes)
			reversed, reversedErr := occurrencemap.Correlate(fixture.Facts, reversedNodes)
			assert.NilError(t, reversedErr)
			assert.DeepEqual(t, reversed, fixture.Expected)
		})
	}
}

func validateFixtureSpans(t *testing.T, fixture correlationFixture) {
	t.Helper()
	for index, fact := range fixture.Facts {
		source, ok := fixture.Sources[fact.File]
		assert.Assert(t, ok, "facts[%d] refers to missing source %q", index, fact.File)
		assert.Assert(t, fact.Span.Start >= 0 && fact.Span.End <= len(source), "facts[%d] is outside %q", index, fact.File)
	}
	for index, node := range fixture.Nodes {
		source, ok := fixture.Sources[node.File]
		assert.Assert(t, ok, "nodes[%d] refers to missing source %q", index, node.File)
		assert.Assert(t, node.Span.Start >= 0 && node.Span.End <= len(source), "nodes[%d] is outside %q", index, node.File)
		for normalizationIndex, normalization := range node.Normalizations {
			assert.Assert(t, normalization.Span.Start >= 0 && normalization.Span.End <= len(source), "nodes[%d].normalizations[%d] is outside %q", index, normalizationIndex, node.File)
		}
	}
}

func TestCorrelationRejectsInvalidNodeContracts(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		nodes []occurrencemap.Node
		want  string
	}{
		{
			name: "duplicate node ID",
			nodes: []occurrencemap.Node{
				{ID: 1, File: "src/a.ts", Span: semanticfacts.Span{Start: 0, End: 1}, SyntaxKind: "KindIdentifier"},
				{ID: 1, File: "src/a.ts", Span: semanticfacts.Span{Start: 2, End: 3}, SyntaxKind: "KindIdentifier"},
			},
			want: "nodes[1].nodeId duplicates nodes[0].nodeId 1 in file \"src/a.ts\"",
		},
		{
			name: "invalid kind alias",
			nodes: []occurrencemap.Node{{
				ID: 1, File: "src/a.ts", Span: semanticfacts.Span{Start: 0, End: 1}, SyntaxKind: "KindIdentifier",
				Normalizations: []occurrencemap.Normalization{{
					Span: semanticfacts.Span{Start: 0, End: 2}, SyntaxKind: "KindIdentifier", Rule: occurrencemap.NormalizationKindAlias,
				}},
			}},
			want: "nodes[0].normalizations[0]: kind-alias requires the canonical span",
		},
		{
			name: "unknown rule",
			nodes: []occurrencemap.Node{{
				ID: 1, File: "src/a.ts", Span: semanticfacts.Span{Start: 0, End: 1}, SyntaxKind: "KindIdentifier",
				Normalizations: []occurrencemap.Normalization{{
					Span: semanticfacts.Span{Start: 0, End: 1}, SyntaxKind: "KindIdentifier", Rule: "guess",
				}},
			}},
			want: "nodes[0].normalizations[0]: unknown normalization rule \"guess\"",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			_, err := occurrencemap.Correlate(nil, test.nodes)
			assert.Error(t, err, test.want)
		})
	}
}

func TestCorrelationAllowsFileLocalNodeIDs(t *testing.T) {
	t.Parallel()

	facts := []semanticfacts.FactRecord{
		{File: "src/a.ts", Span: semanticfacts.Span{Start: 0, End: 1}, SyntaxKind: "KindIdentifier"},
		{File: "src/b.ts", Span: semanticfacts.Span{Start: 0, End: 1}, SyntaxKind: "KindIdentifier"},
	}
	nodes := []occurrencemap.Node{
		{ID: 0, File: "src/a.ts", Span: semanticfacts.Span{Start: 0, End: 1}, SyntaxKind: "KindIdentifier"},
		{ID: 0, File: "src/b.ts", Span: semanticfacts.Span{Start: 0, End: 1}, SyntaxKind: "KindIdentifier"},
	}

	report, err := occurrencemap.Correlate(facts, nodes)
	assert.NilError(t, err)
	assert.DeepEqual(t, report.Mappings, []occurrencemap.Mapping{
		{FactIndex: 0, NodeID: 0, Match: "exact"},
		{FactIndex: 1, NodeID: 0, Match: "exact"},
	})
}

func readFixture(t *testing.T, path string) correlationFixture {
	t.Helper()
	source, err := os.ReadFile(path)
	assert.NilError(t, err)
	var fixture correlationFixture
	assert.NilError(t, json.Unmarshal(source, &fixture))
	return fixture
}

func fixtureNames(paths []string) []string {
	names := make([]string, 0, len(paths))
	for _, path := range paths {
		name := filepath.Base(path)
		names = append(names, name[:len(name)-len(filepath.Ext(name))])
	}
	slices.Sort(names)
	return names
}

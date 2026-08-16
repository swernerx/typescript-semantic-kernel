package semanticfacts_test

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"github.com/microsoft/typescript-go/internal/bundled"
	tsfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	"github.com/microsoft/typescript-go/internal/vfs/vfstest"
	"gotest.tools/v3/assert"
)

func TestSemanticFactsCorpusReferencesResolve(t *testing.T) {
	t.Parallel()

	entries, err := os.ReadDir(corpusRoot)
	assert.NilError(t, err)
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		t.Run(entry.Name(), func(t *testing.T) {
			t.Parallel()

			result, _ := analyzeCorpusFixture(t, entry.Name())
			assert.NilError(t, tsfacts.ValidateResult(result))
			assertCorpusRootsResolve(t, result)
		})
	}
}

func TestSemanticFactsCorpusPreservesRecursiveSharedNodes(t *testing.T) {
	t.Parallel()

	result, _ := analyzeCorpusFixture(t, "core-graph")
	nodeFact := result.Facts[0]
	nodeType := typeByID(t, result, nodeFact.ActualType)
	assert.Equal(t, symbolByID(t, result, nodeType.Symbol).Name, "RecursiveNode")

	var nextProperty tsfacts.SymbolRecord
	for _, propertyID := range nodeType.Properties {
		property := symbolByID(t, result, propertyID)
		if property.Name == "next" {
			nextProperty = property
		}
	}
	assert.Assert(t, nextProperty.ID != "")
	nextType := typeByID(t, result, nextProperty.Type)
	assert.Assert(t, slices.Contains(nextType.Members, nodeType.ID), "recursive edge must return to the interned root")
	assert.Equal(t, countTypeID(result.Types, nodeType.ID), 1, "shared recursive node must be emitted once")
}

func TestSemanticFactsCorpusPreservesGenericSignatureStructure(t *testing.T) {
	t.Parallel()

	result, _ := analyzeCorpusFixture(t, "callables-generics")
	callable := typeByID(t, result, result.Facts[0].ActualType)
	assert.Assert(t, len(callable.CallSignatures) >= 2)
	assert.Assert(t, len(callable.ConstructSignatures) != 0)
	assert.Assert(t, len(callable.IndexSignatures) != 0)

	construct := signatureByID(t, result, callable.ConstructSignatures[0])
	assert.Assert(t, len(construct.TypeParameters) != 0)
	typeParameter := typeByID(t, result, construct.TypeParameters[0])
	assert.Assert(t, typeParameter.Constraint != "")
	assert.Assert(t, typeParameter.Default != "")

	instantiated := typeByID(t, result, result.Facts[1].ActualType)
	assert.Assert(t, len(instantiated.CallSignatures) != 0)
	signature := signatureByID(t, result, instantiated.CallSignatures[0])
	assert.Assert(t, signature.Target != "")
	assert.Equal(t, len(signature.TypeArguments), 1)
}

func TestSemanticFactsCorpusPreservesAliasesAndOccurrenceViews(t *testing.T) {
	t.Parallel()

	result, manifest := analyzeCorpusFixture(t, "occurrence-contexts")
	aliasFact := factForProof(t, result, manifest, "import alias")
	alias := symbolByID(t, result, aliasFact.Symbol)
	assert.Assert(t, alias.AliasedSymbol != "")

	narrowed := factForProof(t, result, manifest, "control-flow narrowing")
	assert.Assert(t, narrowed.NarrowedType != "")
	assert.Assert(t, narrowed.DeclaredType != "")
	assert.Assert(t, narrowed.NarrowedType != narrowed.DeclaredType)

	contextual := factForProof(t, result, manifest, "contextual literal type")
	assert.Assert(t, contextual.ContextualType != "")
	assert.Assert(t, contextual.ContextualType != contextual.ActualType)
}

func TestSemanticFactsCorpusPreservesRecoveryAndTruncation(t *testing.T) {
	t.Parallel()

	result, _ := analyzeCorpusFixture(t, "recovery-budgets")
	assert.Assert(t, result.Header.Budgets.Truncated)
	assert.Assert(t, slices.ContainsFunc(result.Facts, func(fact tsfacts.FactRecord) bool {
		return fact.Recovered
	}))
	assert.Assert(t, slices.ContainsFunc(result.Types, func(record tsfacts.TypeRecord) bool {
		return slices.ContainsFunc(record.Issues, func(issue tsfacts.GraphIssue) bool {
			return issue.Code == tsfacts.GraphIssueMaxTypeDepth || issue.Code == tsfacts.GraphIssueMaxTypeNodes
		})
	}))
	assert.NilError(t, tsfacts.ValidateResult(result))
}

func TestSemanticFactsCorpusNamesUnsupportedCapabilityGaps(t *testing.T) {
	t.Parallel()

	options, request, _ := loadCorpusFixture(t, "advanced-types")
	request.RequiredCapabilities = append(request.RequiredCapabilities, "types.future-object-form")
	_, err := tsfacts.Analyze(t.Context(), options, request)
	assert.ErrorContains(t, err, `unsupported required capability "types.future-object-form"`)
}

func analyzeCorpusFixture(t *testing.T, caseName string) (*tsfacts.Result, corpusManifest) {
	t.Helper()
	options, request, manifest := loadCorpusFixture(t, caseName)
	result, err := tsfacts.Analyze(t.Context(), options, request)
	assert.NilError(t, err)
	return result, manifest
}

func loadCorpusFixture(t *testing.T, caseName string) (tsfacts.AnalyzerOptions, tsfacts.Request, corpusManifest) {
	t.Helper()
	directory := filepath.Join(corpusRoot, caseName)
	manifest := readCorpusManifest(t, directory)
	projectFiles := readCorpusProject(t, directory)
	selections := make([]tsfacts.Selection, 0, len(manifest.Selections))
	for _, selection := range manifest.Selections {
		source := projectFiles["/project/"+filepath.ToSlash(selection.File)]
		start := nthCorpusOccurrence(t, source, selection.Text, selection.Occurrence)
		selections = append(selections, tsfacts.Selection{
			File:  filepath.ToSlash(selection.File),
			Start: start,
			End:   start + len(selection.Text),
		})
	}
	return tsfacts.AnalyzerOptions{
			CurrentDirectory:   "/project",
			FS:                 bundled.WrapFS(vfstest.FromMap(projectFiles, true)),
			DefaultLibraryPath: bundled.LibPath(),
		}, tsfacts.Request{
			SchemaVersion:        tsfacts.SchemaVersion,
			RequiredCapabilities: manifest.Capabilities,
			Budgets:              manifest.Budgets,
			Project:              manifest.Project,
			Selections:           selections,
		}, manifest
}

func assertCorpusRootsResolve(t *testing.T, result *tsfacts.Result) {
	t.Helper()
	for _, fact := range result.Facts {
		assert.Assert(t, fact.ActualType != "")
		typeByID(t, result, fact.ActualType)
		if fact.Symbol != "" {
			symbolByID(t, result, fact.Symbol)
		}
		for _, declarationID := range fact.Declarations {
			assert.Assert(t, slices.ContainsFunc(result.Declarations, func(record tsfacts.DeclarationRecord) bool {
				return record.ID == declarationID
			}))
		}
	}
}

func factForProof(t *testing.T, result *tsfacts.Result, manifest corpusManifest, proof string) tsfacts.FactRecord {
	t.Helper()
	for index, selection := range manifest.Selections {
		if strings.Contains(selection.Proves, proof) {
			return result.Facts[index]
		}
	}
	t.Fatalf("corpus case %q has no proof containing %q", manifest.Name, proof)
	return tsfacts.FactRecord{}
}

func countTypeID(records []tsfacts.TypeRecord, id tsfacts.TypeID) int {
	count := 0
	for _, record := range records {
		if record.ID == id {
			count++
		}
	}
	return count
}

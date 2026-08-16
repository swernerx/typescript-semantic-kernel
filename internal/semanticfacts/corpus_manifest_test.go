package semanticfacts_test

import (
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strings"
	"testing"

	"github.com/microsoft/typescript-go/internal/bundled"
	"github.com/microsoft/typescript-go/internal/json"
	tsfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	"github.com/microsoft/typescript-go/internal/vfs/vfstest"
	"gotest.tools/v3/assert"
)

const corpusRoot = "testdata/corpus/v0"

var requiredCorpusCoverage = []string{
	"aliases",
	"arrays",
	"as-const",
	"classes",
	"conditional-types",
	"constraints",
	"constructors",
	"contextual-typing",
	"control-flow-narrowing",
	"declaration-use-views",
	"defaults",
	"functions",
	"generics",
	"imports",
	"indexed-access",
	"interfaces",
	"intersections",
	"jsx-props",
	"keyof",
	"literals",
	"mapped-types",
	"objects",
	"overloads",
	"primitives",
	"recursion",
	"recovery",
	"satisfies",
	"sharing",
	"string-mapping",
	"syntax-errors",
	"template-literal-types",
	"truncation-pressure",
	"tuples",
	"type-assertions",
	"type-errors",
	"typeof",
	"unions",
}

type corpusManifest struct {
	Name         string               `json:"name"`
	Description  string               `json:"description"`
	Coverage     []string             `json:"coverage"`
	Combined     []string             `json:"combinedEvidence"`
	Project      string               `json:"project"`
	Capabilities []string             `json:"capabilities"`
	Budgets      tsfacts.BudgetLimits `json:"budgets"`
	Selections   []corpusSelection    `json:"selections"`
	Expectations corpusExpectations   `json:"expectations"`
}

type corpusSelection struct {
	File       string `json:"file"`
	Text       string `json:"text"`
	Occurrence int    `json:"occurrence"`
	Proves     string `json:"proves"`
}

type corpusExpectations struct {
	Recovery   *bool `json:"recovery"`
	Truncation *bool `json:"truncation"`
}

type corpusCase struct {
	directory string
	manifest  corpusManifest
}

func TestSemanticFactsCorpusManifests(t *testing.T) {
	t.Parallel()

	entries, err := os.ReadDir(corpusRoot)
	assert.NilError(t, err)

	covered := make(map[string]string)
	combinedEvidenceFound := false
	cases := make([]corpusCase, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		caseDirectory := filepath.Join(corpusRoot, entry.Name())
		manifest := readCorpusManifest(t, caseDirectory)
		assert.Equal(t, manifest.Name, entry.Name())
		for _, capability := range manifest.Coverage {
			assert.Assert(t, capability != "")
			if previous := covered[capability]; previous != "" {
				t.Fatalf("coverage %q is claimed by both %q and %q", capability, previous, manifest.Name)
			}
			covered[capability] = manifest.Name
		}
		if len(manifest.Combined) != 0 {
			assert.DeepEqual(t, manifest.Combined, []string{
				"control-flow-narrowing",
				"overloads",
				"recursion",
				"sharing",
				"truncation-pressure",
			})
			combinedEvidenceFound = true
		}
		cases = append(cases, corpusCase{directory: caseDirectory, manifest: manifest})
	}

	assert.Assert(t, len(cases) >= 6, "corpus must remain split into locally diagnosable cases")
	assert.Assert(t, combinedEvidenceFound, "one fixture must combine sharing, recursion, overloads, narrowing, and truncation")
	for _, capability := range requiredCorpusCoverage {
		assert.Assert(t, covered[capability] != "", "missing corpus coverage %q", capability)
	}

	for _, corpusCase := range cases {
		t.Run(corpusCase.manifest.Name, func(t *testing.T) {
			t.Parallel()

			caseDirectory := corpusCase.directory
			manifest := corpusCase.manifest
			assert.Assert(t, manifest.Description != "")
			assert.Assert(t, len(manifest.Coverage) != 0)
			assert.Assert(t, len(manifest.Selections) != 0)
			readme, readmeErr := os.ReadFile(filepath.Join(caseDirectory, "README.md"))
			assert.NilError(t, readmeErr)
			assert.Assert(t, len(strings.TrimSpace(string(readme))) != 0)

			projectFiles := readCorpusProject(t, caseDirectory)
			selections := make([]tsfacts.Selection, 0, len(manifest.Selections))
			for _, selection := range manifest.Selections {
				assert.Assert(t, selection.Proves != "")
				sourcePath := "/project/" + filepath.ToSlash(selection.File)
				source, ok := projectFiles[sourcePath]
				assert.Assert(t, ok, "selection source %q is missing", selection.File)
				start := nthCorpusOccurrence(t, source, selection.Text, selection.Occurrence)
				selections = append(selections, tsfacts.Selection{
					File:  filepath.ToSlash(selection.File),
					Start: start,
					End:   start + len(selection.Text),
				})
			}

			result, analyzeErr := tsfacts.Analyze(t.Context(), tsfacts.AnalyzerOptions{
				CurrentDirectory:   "/project",
				FS:                 bundled.WrapFS(vfstest.FromMap(projectFiles, true)),
				DefaultLibraryPath: bundled.LibPath(),
			}, tsfacts.Request{
				SchemaVersion:        tsfacts.SchemaVersion,
				RequiredCapabilities: manifest.Capabilities,
				Budgets:              manifest.Budgets,
				Project:              manifest.Project,
				Selections:           selections,
			})
			assert.NilError(t, analyzeErr)
			assert.NilError(t, tsfacts.ValidateResult(result))
			assert.Equal(t, len(result.Facts), len(selections))

			recovered := false
			truncated := result.Header.Budgets.Truncated
			for _, fact := range result.Facts {
				recovered = recovered || fact.Recovered
				truncated = truncated || fact.Truncated
			}
			if manifest.Expectations.Recovery != nil {
				assert.Equal(t, recovered, *manifest.Expectations.Recovery)
			}
			if manifest.Expectations.Truncation != nil {
				assert.Equal(t, truncated, *manifest.Expectations.Truncation)
			}
		})
	}
}

func readCorpusManifest(t *testing.T, directory string) corpusManifest {
	t.Helper()
	source, err := os.ReadFile(filepath.Join(directory, "case.json"))
	assert.NilError(t, err)
	var manifest corpusManifest
	assert.NilError(t, json.Unmarshal(source, &manifest))
	sortedCoverage := append([]string(nil), manifest.Coverage...)
	sort.Strings(sortedCoverage)
	assert.Assert(t, slices.Equal(manifest.Coverage, sortedCoverage), "coverage in %s must be sorted", directory)
	sortedCapabilities := append([]string(nil), manifest.Capabilities...)
	sort.Strings(sortedCapabilities)
	assert.Assert(t, slices.Equal(manifest.Capabilities, sortedCapabilities), "capabilities in %s must be sorted", directory)
	return manifest
}

func readCorpusProject(t *testing.T, directory string) map[string]string {
	t.Helper()
	files := make(map[string]string)
	err := filepath.WalkDir(directory, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() || entry.Name() == "case.json" || entry.Name() == "README.md" {
			return nil
		}
		relative, relativeErr := filepath.Rel(directory, path)
		if relativeErr != nil {
			return relativeErr
		}
		source, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		assert.Assert(t, len(source) <= 4096, "corpus source %s is too large for local diagnosis", path)
		files["/project/"+filepath.ToSlash(relative)] = string(source)
		return nil
	})
	assert.NilError(t, err)
	return files
}

func nthCorpusOccurrence(t *testing.T, source string, text string, occurrence int) int {
	t.Helper()
	offset := 0
	for index := 0; index <= occurrence; index++ {
		match := strings.Index(source[offset:], text)
		if match == -1 {
			t.Fatalf("selection %q occurrence %d was not found", text, occurrence)
		}
		start := offset + match
		if index == occurrence {
			return start
		}
		offset = start + len(text)
	}
	t.Fatal("unreachable")
	return 0
}

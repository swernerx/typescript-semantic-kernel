package semanticfacts_test

import (
	"bytes"
	"fmt"
	"slices"
	"testing"

	tsfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	transport "github.com/microsoft/typescript-go/internal/tsfacts"
	"gotest.tools/v3/assert"
)

func TestAnalyzeExportsAdvancedTypeGraph(t *testing.T) {
	t.Parallel()
	const source = "" +
		"export {};\n" +
		"type Uppercase<S extends string> = intrinsic;\n" +
		"type Capitalize<S extends string> = intrinsic;\n" +
		"type Conditional<T> = T extends { value: infer U } ? U : never;\n" +
		"type Mapped<T> = { readonly [K in keyof T as `get${Capitalize<string & K>}`]?: T[K] };\n" +
		"type Indexed<T, K extends keyof T> = T[K];\n" +
		"type Key<T> = keyof T;\n" +
		"type Template<T extends string> = `event:${Uppercase<T>}`;\n" +
		"declare function probe<T extends string, U extends object>(): {\n" +
		"  conditional: Conditional<T>;\n" +
		"  mapped: Mapped<U>;\n" +
		"  indexed: Indexed<U, keyof U>;\n" +
		"  key: Key<U>;\n" +
		"  template: Template<T>;\n" +
		"};\n" +
		"probe;\n"
	request := tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		RequiredCapabilities: []string{
			tsfacts.CapabilityAdvancedTypes,
			tsfacts.CapabilityCoreCompositeTypes,
		},
		Project: "tsconfig.json",
		Selections: []tsfacts.Selection{
			selectionAt(source, "Conditional", 0),
			selectionAt(source, "Mapped", 0),
			selectionAt(source, "Indexed", 0),
			selectionAt(source, "Key", 0),
			selectionAt(source, "Template", 0),
		},
	}
	result := analyzeFixture(t, source, request)
	assert.NilError(t, tsfacts.ValidateResult(result))

	byKind := make(map[string][]tsfacts.TypeRecord)
	for _, record := range result.Types {
		byKind[record.TypeKind] = append(byKind[record.TypeKind], record)
	}
	for _, kind := range []string{"conditional", "mapped", "indexed_access", "index", "template_literal", "string_mapping"} {
		assert.Assert(t, len(byKind[kind]) != 0, "missing %s in %s", kind, describeTypeKinds(result.Types))
	}

	conditional := byKind["conditional"][0]
	assert.Assert(t, conditional.Conditional != nil)
	assert.Assert(t, conditional.Conditional.CheckType != "")
	assert.Assert(t, conditional.Conditional.ExtendsType != "")
	assert.Assert(t, conditional.Conditional.TrueType != "")
	assert.Assert(t, conditional.Conditional.FalseType != "")
	assert.Equal(t, len(conditional.Conditional.InferTypeParameters), 1)
	assert.Assert(t, conditional.Conditional.Distributive)

	mapped := byKind["mapped"][0]
	assert.Assert(t, mapped.Mapped != nil)
	assert.Assert(t, mapped.Mapped.TypeParameter != "")
	assert.Assert(t, mapped.Mapped.ConstraintType != "")
	assert.Assert(t, mapped.Mapped.NameType != "")
	assert.Assert(t, mapped.Mapped.TemplateType != "")
	assert.Equal(t, mapped.Mapped.ReadonlyModifier, "add")
	assert.Equal(t, mapped.Mapped.OptionalModifier, "add")

	indexed := byKind["indexed_access"][0]
	assert.Assert(t, indexed.IndexedAccess != nil)
	assert.Assert(t, indexed.IndexedAccess.ObjectType != "")
	assert.Assert(t, indexed.IndexedAccess.IndexType != "")

	template := byKind["template_literal"][0]
	assert.Assert(t, template.TemplateLiteral != nil)
	assert.Equal(t, len(template.TemplateLiteral.Texts), len(template.TemplateLiteral.Types)+1)
	expectedRoots := []string{"conditional", "mapped", "indexed_access", "index", "template_literal"}
	for index, fact := range result.Facts {
		assert.Equal(t, typeByID(t, result, fact.ActualType).TypeKind, expectedRoots[index], "fact=%+v types=%s", fact, describeTypeKinds(result.Types))
		assert.Assert(t, !fact.Recovered)
	}

	repeated := analyzeFixture(t, source, request)
	var firstOutput bytes.Buffer
	var repeatedOutput bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&firstOutput, result))
	assert.NilError(t, transport.WriteJSONLines(&repeatedOutput, repeated))
	assert.Equal(t, firstOutput.String(), repeatedOutput.String())
}

func TestAnalyzeKeepsAdvancedRecoveryAndBudgetCutoffsInspectable(t *testing.T) {
	t.Parallel()
	const broken = "type Broken<T> = T extends string ? Missing<T> : never; Broken<string>;"
	recovered := analyzeFixture(t, broken, tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityAdvancedTypes},
		Project:              "tsconfig.json",
		Selections:           []tsfacts.Selection{selectionAt(broken, "Broken", 0)},
	})
	assert.NilError(t, tsfacts.ValidateResult(recovered))
	assert.Equal(t, len(recovered.Facts), 1)
	assert.Assert(t, recovered.Facts[0].Recovered)
	assert.Assert(t, slices.ContainsFunc(recovered.Types, func(record tsfacts.TypeRecord) bool {
		return record.TypeKind == "error" && record.State == tsfacts.EntityStateError
	}))

	const deep = "type Deep<T> = T extends string ? `${T}-${T}` : never; Deep<string>;"
	bounded := analyzeFixture(t, deep, tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityAdvancedTypes},
		Budgets:              tsfacts.BudgetLimits{MaxTypeNodes: 64, MaxTypeDepth: 1},
		Project:              "tsconfig.json",
		Selections:           []tsfacts.Selection{selectionAt(deep, "Deep", 0)},
	})
	assert.NilError(t, tsfacts.ValidateResult(bounded))
	assert.Assert(t, bounded.Header.Budgets.Truncated)
	assert.Assert(t, bounded.Facts[0].Truncated)
	assert.Assert(t, slices.ContainsFunc(bounded.Types, func(record tsfacts.TypeRecord) bool {
		return slices.ContainsFunc(record.Issues, func(issue tsfacts.GraphIssue) bool {
			return issue.Code == tsfacts.GraphIssueMaxTypeDepth
		})
	}))
}

func TestAnalyzeInternsRecursiveConditionalGraphOnce(t *testing.T) {
	t.Parallel()
	const source = "type Recursive<T> = T extends string ? { next: Recursive<T> } : never; declare function make<T extends string>(): Recursive<T>; make;"
	result := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		RequiredCapabilities: []string{
			tsfacts.CapabilityAdvancedTypes,
			tsfacts.CapabilityCoreCompositeTypes,
			tsfacts.CapabilityGraphReferences,
			tsfacts.CapabilityGraphSignatures,
		},
		Project:    "tsconfig.json",
		Selections: []tsfacts.Selection{selectionAt(source, "make", 1)},
	})
	assert.NilError(t, tsfacts.ValidateResult(result))

	foundCycle := false
	for _, record := range result.Types {
		if record.TypeKind != "conditional" || record.Conditional == nil {
			continue
		}
		trueBranch := typeByID(t, result, record.Conditional.TrueType)
		if trueBranch.TypeKind != "object" || len(trueBranch.Properties) != 1 {
			continue
		}
		next := symbolByID(t, result, trueBranch.Properties[0])
		if next.Type == record.ID {
			foundCycle = true
		}
	}
	assert.Assert(t, foundCycle, "recursive conditional cycle was not preserved: %s", describeTypeKinds(result.Types))
}

func TestAnalyzeNegotiatesAdvancedTypeVariants(t *testing.T) {
	t.Parallel()
	const source = "type Conditional<T> = T extends string ? T : never; declare const value: Conditional<string>; value;"
	selection := selectionAt(source, "Conditional", 0)
	legacy := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion: tsfacts.SchemaVersion,
		Project:       "tsconfig.json",
		Selections:    []tsfacts.Selection{selection},
	})
	assert.Assert(t, slices.ContainsFunc(legacy.Types, func(record tsfacts.TypeRecord) bool {
		return record.TypeKind == "opaque" && record.State == tsfacts.EntityStateUnsupported
	}))

	negotiated := analyzeFixture(t, source, tsfacts.Request{
		SchemaVersion:        tsfacts.SchemaVersion,
		RequiredCapabilities: []string{tsfacts.CapabilityAdvancedTypes},
		Project:              "tsconfig.json",
		Selections:           []tsfacts.Selection{selection},
	})
	assert.Assert(t, slices.ContainsFunc(negotiated.Types, func(record tsfacts.TypeRecord) bool {
		return record.TypeKind == "conditional"
	}))
}

func describeTypeKinds(records []tsfacts.TypeRecord) string {
	kinds := make([]string, 0, len(records))
	for _, record := range records {
		kinds = append(kinds, fmt.Sprintf("%s=%s/%s:%v", record.ID, record.TypeKind, record.State, record.Issues))
	}
	return fmt.Sprint(kinds)
}

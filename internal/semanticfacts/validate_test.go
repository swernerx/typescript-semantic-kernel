package semanticfacts_test

import (
	"bytes"
	"strings"
	"testing"

	tsfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
	transport "github.com/microsoft/typescript-go/internal/tsfacts"
	"gotest.tools/v3/assert"
)

func TestValidateResultAllowsSharedAndCyclicGraphEdges(t *testing.T) {
	t.Parallel()
	result := cyclicGraphResult()

	assert.NilError(t, tsfacts.ValidateResult(result))
	var output bytes.Buffer
	assert.NilError(t, transport.WriteJSONLines(&output, result))
	encoded := output.String()
	assert.Equal(t, strings.Count(encoded, `"id":"type:1"`), 1)
	assert.Assert(t, strings.Contains(encoded, `"record":"signature"`))
	assert.Assert(t, strings.Index(encoded, `"record":"symbol"`) < strings.Index(encoded, `"record":"signature"`))
}

func TestValidateResultRejectsDanglingGraphEdge(t *testing.T) {
	t.Parallel()
	result := cyclicGraphResult()
	result.Types[0].Target = "type:999"

	err := tsfacts.ValidateResult(result)
	assert.ErrorContains(t, err, `type type:1 references missing type "type:999"`)
}

func TestValidateResultRejectsUnknownVariant(t *testing.T) {
	t.Parallel()
	result := cyclicGraphResult()
	result.Types[0].TypeKind = "future-magic"

	err := tsfacts.ValidateResult(result)
	assert.ErrorContains(t, err, `unknown typeKind "future-magic"`)
}

func TestValidateResultRejectsDuplicateIdentity(t *testing.T) {
	t.Parallel()
	result := cyclicGraphResult()
	result.Types = append(result.Types, result.Types[0])

	err := tsfacts.ValidateResult(result)
	assert.ErrorContains(t, err, `duplicate type id "type:1"`)
}

func TestValidateResultRejectsIncompleteEdgeFromCompleteEntity(t *testing.T) {
	t.Parallel()
	result := cyclicGraphResult()
	result.Types[1].State = tsfacts.EntityStateTruncated
	result.Types[1].Issues = []tsfacts.GraphIssue{{Code: tsfacts.GraphIssueUnsupportedStructure}}
	result.Types[1].Complete = false
	result.Types[1].Truncated = true

	err := tsfacts.ValidateResult(result)
	assert.ErrorContains(t, err, `complete symbol symbol:1 references truncated type "type:2"`)
}

func TestValidateResultRejectsUnknownEntityState(t *testing.T) {
	t.Parallel()
	result := cyclicGraphResult()
	result.Types[0].State = "future-state"
	result.Types[0].Complete = false

	err := tsfacts.ValidateResult(result)
	assert.ErrorContains(t, err, `type "type:1" has unknown state "future-state"`)
}

func TestValidateResultRejectsIncoherentTypeViewState(t *testing.T) {
	t.Parallel()
	result := cyclicGraphResult()
	result.Files = []tsfacts.FileRecord{{Record: "file", ID: "src/example.ts", Origin: "project"}}
	result.Facts = []tsfacts.FactRecord{{
		Record:         "fact",
		File:           "src/example.ts",
		ActualType:     "type:1",
		TypeAtLocation: "type:1",
		ContextualType: "type:2",
		TypeViewStates: tsfacts.TypeViewStates{
			Actual:     tsfacts.TypeViewAvailable,
			Contextual: tsfacts.TypeViewUnavailable,
			Widened:    tsfacts.TypeViewSameAsActual,
			Apparent:   tsfacts.TypeViewSameAsActual,
			Declared:   tsfacts.TypeViewInapplicable,
		},
	}}

	err := tsfacts.ValidateResult(result)
	assert.ErrorContains(t, err, `contextual type view state "unavailable" must omit its root`)
}

func cyclicGraphResult() *tsfacts.Result {
	return &tsfacts.Result{
		Header: tsfacts.HeaderRecord{
			Record:        "header",
			SchemaVersion: tsfacts.SchemaVersion,
			Capabilities:  tsfacts.SupportedCapabilities(),
			Budgets: tsfacts.BudgetReport{
				Limits:        tsfacts.BudgetLimits{MaxTypeNodes: tsfacts.DefaultMaxTypeNodes, MaxTypeDepth: tsfacts.DefaultMaxTypeDepth},
				TypeNodesUsed: 3,
			},
		},
		Types: []tsfacts.TypeRecord{
			{
				Record:     "type",
				ID:         "type:1",
				TypeKind:   "object",
				Display:    "Node",
				Flags:      []string{"Object"},
				Properties: []tsfacts.SymbolID{"symbol:1"},
				State:      tsfacts.EntityStateComplete,
				Complete:   true,
			},
			{
				Record:         "type",
				ID:             "type:2",
				TypeKind:       "callable",
				Display:        "(next: Node) => Node",
				Flags:          []string{"Object"},
				CallSignatures: []tsfacts.SignatureID{"signature:1"},
				State:          tsfacts.EntityStateComplete,
				Complete:       true,
			},
			{
				Record:     "type",
				ID:         "type:3",
				TypeKind:   "type_parameter",
				Display:    "T",
				Flags:      []string{"TypeParameter"},
				Constraint: "type:1",
				Default:    "type:1",
				State:      tsfacts.EntityStateComplete,
				Complete:   true,
			},
		},
		Symbols: []tsfacts.SymbolRecord{
			{
				Record:   "symbol",
				ID:       "symbol:1",
				Name:     "next",
				Roles:    []string{"property"},
				Type:     "type:2",
				Members:  []tsfacts.SymbolID{"symbol:1"},
				State:    tsfacts.EntityStateComplete,
				Complete: true,
			},
		},
		Signatures: []tsfacts.SignatureRecord{
			{
				Record:         "signature",
				ID:             "signature:1",
				SignatureKind:  "call",
				TypeParameters: []tsfacts.TypeID{"type:3"},
				Parameters:     []tsfacts.SymbolID{"symbol:1"},
				ReturnType:     "type:1",
				State:          tsfacts.EntityStateComplete,
				Complete:       true,
			},
		},
	}
}

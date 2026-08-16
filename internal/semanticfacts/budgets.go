package semanticfacts

import (
	"fmt"
	"slices"
)

const (
	DefaultMaxTypeNodes = 4096
	DefaultMaxTypeDepth = 32

	CapabilityGraphReferences    = "graph.references"
	CapabilityGraphSignatures    = "graph.signatures"
	CapabilityTypeBudgets        = "limits.type-graph"
	CapabilityFileWideRoots      = "occurrence.file-wide"
	CapabilityOccurrenceViews    = "occurrence.type-views"
	CapabilityExplicitStates     = "protocol.explicit-states"
	CapabilityCanonicalFixtures  = "protocol.fixtures.v0"
	CapabilityAdvancedTypes      = "types.advanced"
	CapabilityCoreCompositeTypes = "types.core-composite"

	GraphIssueCheckerError                  = "checker-error"
	GraphIssueMaxTypeDepth                  = "max-type-depth"
	GraphIssueMaxTypeNodes                  = "max-type-nodes"
	GraphIssueReferencedIncompleteSignature = "referenced-incomplete-signature"
	GraphIssueReferencedIncompleteSymbol    = "referenced-incomplete-symbol"
	GraphIssueReferencedIncompleteType      = "referenced-incomplete-type"
	GraphIssueMissingTypeEdge               = "missing-type-edge"
	GraphIssueUnsupportedStructure          = "unsupported-structure"
	GraphIssueUnsupportedTypeForm           = "unsupported-type-form"
	GraphIssueUnrepresentableDecl           = "unrepresentable-declaration"
	GraphIssueUnresolvedAlias               = "unresolved-alias"
	GraphIssueReferencedAlias               = "referenced-incomplete-alias"
)

var supportedCapabilities = []string{
	CapabilityGraphReferences,
	CapabilityGraphSignatures,
	CapabilityTypeBudgets,
	CapabilityFileWideRoots,
	CapabilityOccurrenceViews,
	CapabilityExplicitStates,
	CapabilityCanonicalFixtures,
	CapabilityAdvancedTypes,
	CapabilityCoreCompositeTypes,
}

func SupportedCapabilities() []string {
	return slices.Clone(supportedCapabilities)
}

func normalizeBudgetLimits(requested BudgetLimits) (BudgetLimits, error) {
	limits := requested
	if limits.MaxTypeNodes == 0 {
		limits.MaxTypeNodes = DefaultMaxTypeNodes
	}
	if limits.MaxTypeDepth == 0 {
		limits.MaxTypeDepth = DefaultMaxTypeDepth
	}
	if limits.MaxTypeNodes < 1 {
		return BudgetLimits{}, fmt.Errorf("budgets.maxTypeNodes must be positive; got %d", limits.MaxTypeNodes)
	}
	if limits.MaxTypeDepth < 1 {
		return BudgetLimits{}, fmt.Errorf("budgets.maxTypeDepth must be positive; got %d", limits.MaxTypeDepth)
	}
	return limits, nil
}

func validateRequiredCapabilities(required []string) error {
	known := make(map[string]struct{}, len(supportedCapabilities))
	for _, capability := range supportedCapabilities {
		known[capability] = struct{}{}
	}
	seen := make(map[string]struct{}, len(required))
	for index, capability := range required {
		if _, duplicate := seen[capability]; duplicate {
			return fmt.Errorf("requiredCapabilities[%d] duplicates %q", index, capability)
		}
		seen[capability] = struct{}{}
		if _, ok := known[capability]; !ok {
			return fmt.Errorf("unsupported required capability %q", capability)
		}
	}
	return nil
}

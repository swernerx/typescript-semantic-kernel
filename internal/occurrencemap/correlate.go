// Package occurrencemap defines the consumer-neutral contract for correlating
// semantic-facts occurrences with syntax-tree node IDs.
package occurrencemap

import (
	"cmp"
	"fmt"
	"slices"

	semanticfacts "github.com/microsoft/typescript-go/internal/semanticfacts"
)

const ContractVersion = 1

type NodeID uint32

type NormalizationRule string

const (
	NormalizationKindAlias         NormalizationRule = "kind-alias"
	NormalizationProtocolInnerSpan NormalizationRule = "protocol-inner-span"
	NormalizationProtocolOuterSpan NormalizationRule = "protocol-outer-span"
)

type Node struct {
	ID             NodeID             `json:"nodeId"`
	File           string             `json:"file"`
	Span           semanticfacts.Span `json:"span"`
	SyntaxKind     string             `json:"syntaxKind"`
	Normalizations []Normalization    `json:"normalizations,omitzero"`
}

// Normalization is an additional protocol-side anchor for one syntax node.
// Span and SyntaxKind use semantic-facts coordinates and names; Rule explains
// why the anchor differs from the node's canonical parser-side identity.
type Normalization struct {
	Span       semanticfacts.Span `json:"span"`
	SyntaxKind string             `json:"syntaxKind"`
	Rule       NormalizationRule  `json:"rule"`
}

type Mapping struct {
	FactIndex int               `json:"factIndex"`
	NodeID    NodeID            `json:"nodeId"`
	Match     string            `json:"match"`
	Rule      NormalizationRule `json:"rule,omitzero"`
}

type Candidate struct {
	NodeID NodeID            `json:"nodeId"`
	Rule   NormalizationRule `json:"rule,omitzero"`
}

type Diagnostic struct {
	Code       string             `json:"code"`
	FactIndex  int                `json:"factIndex"`
	File       string             `json:"file"`
	Span       semanticfacts.Span `json:"span"`
	SyntaxKind string             `json:"syntaxKind"`
	Candidates []Candidate        `json:"candidates,omitzero"`
}

type Metrics struct {
	Facts          int `json:"facts"`
	Mapped         int `json:"mapped"`
	Exact          int `json:"exact"`
	Normalized     int `json:"normalized"`
	Unmapped       int `json:"unmapped"`
	MultiplyMapped int `json:"multiplyMapped"`
}

type KindCoverage struct {
	SyntaxKind string  `json:"syntaxKind"`
	Metrics    Metrics `json:"metrics"`
}

type Report struct {
	ContractVersion int            `json:"contractVersion"`
	Mappings        []Mapping      `json:"mappings"`
	Diagnostics     []Diagnostic   `json:"diagnostics"`
	Summary         Metrics        `json:"summary"`
	BySyntaxKind    []KindCoverage `json:"bySyntaxKind"`
}

type key struct {
	file       string
	start      int
	end        int
	syntaxKind string
}

type nodeIdentity struct {
	file string
	id   NodeID
}

// Correlate attaches facts to opaque consumer node IDs. Canonical node spans
// are indexed separately so an exact match always wins over normalization.
// Ambiguous matches are diagnosed and deliberately left unattached.
func Correlate(facts []semanticfacts.FactRecord, nodes []Node) (Report, error) {
	exact := make(map[key]map[NodeID]struct{}, len(nodes))
	normalized := make(map[key]map[NodeID]NormalizationRule)
	seenNodeIDs := make(map[nodeIdentity]int, len(nodes))
	for index, node := range nodes {
		identity := nodeIdentity{file: node.File, id: node.ID}
		if previous, ok := seenNodeIDs[identity]; ok {
			return Report{}, fmt.Errorf("nodes[%d].nodeId duplicates nodes[%d].nodeId %d in file %q", index, previous, node.ID, node.File)
		}
		seenNodeIDs[identity] = index
		if err := validateIdentity(node.File, node.Span, node.SyntaxKind); err != nil {
			return Report{}, fmt.Errorf("nodes[%d]: %w", index, err)
		}
		canonicalKey := identityKey(node.File, node.Span, node.SyntaxKind)
		if exact[canonicalKey] == nil {
			exact[canonicalKey] = make(map[NodeID]struct{})
		}
		exact[canonicalKey][node.ID] = struct{}{}

		for normalizationIndex, normalization := range node.Normalizations {
			if err := validateNormalization(node, normalization); err != nil {
				return Report{}, fmt.Errorf("nodes[%d].normalizations[%d]: %w", index, normalizationIndex, err)
			}
			normalizedKey := identityKey(node.File, normalization.Span, normalization.SyntaxKind)
			if normalized[normalizedKey] == nil {
				normalized[normalizedKey] = make(map[NodeID]NormalizationRule)
			}
			previous, exists := normalized[normalizedKey][node.ID]
			if !exists || normalization.Rule < previous {
				normalized[normalizedKey][node.ID] = normalization.Rule
			}
		}
	}

	report := Report{
		ContractVersion: ContractVersion,
		Mappings:        make([]Mapping, 0, len(facts)),
		Diagnostics:     make([]Diagnostic, 0),
	}
	coverage := make(map[string]*Metrics)
	for factIndex, fact := range facts {
		if err := validateIdentity(fact.File, fact.Span, fact.SyntaxKind); err != nil {
			return Report{}, fmt.Errorf("facts[%d]: %w", factIndex, err)
		}
		metrics := coverage[fact.SyntaxKind]
		if metrics == nil {
			metrics = &Metrics{}
			coverage[fact.SyntaxKind] = metrics
		}
		metrics.Facts++
		report.Summary.Facts++

		factKey := identityKey(fact.File, fact.Span, fact.SyntaxKind)
		if candidates := exactCandidates(exact[factKey]); len(candidates) != 0 {
			if len(candidates) == 1 {
				report.Mappings = append(report.Mappings, Mapping{FactIndex: factIndex, NodeID: candidates[0].NodeID, Match: "exact"})
				metrics.Mapped++
				metrics.Exact++
				report.Summary.Mapped++
				report.Summary.Exact++
			} else {
				report.Diagnostics = append(report.Diagnostics, diagnosticFor(factIndex, fact, "multiply-mapped", candidates))
				metrics.MultiplyMapped++
				report.Summary.MultiplyMapped++
			}
			continue
		}

		if candidates := normalizedCandidates(normalized[factKey]); len(candidates) != 0 {
			if len(candidates) == 1 {
				report.Mappings = append(report.Mappings, Mapping{FactIndex: factIndex, NodeID: candidates[0].NodeID, Match: "normalized", Rule: candidates[0].Rule})
				metrics.Mapped++
				metrics.Normalized++
				report.Summary.Mapped++
				report.Summary.Normalized++
			} else {
				report.Diagnostics = append(report.Diagnostics, diagnosticFor(factIndex, fact, "multiply-mapped", candidates))
				metrics.MultiplyMapped++
				report.Summary.MultiplyMapped++
			}
			continue
		}

		report.Diagnostics = append(report.Diagnostics, diagnosticFor(factIndex, fact, "unmapped", nil))
		metrics.Unmapped++
		report.Summary.Unmapped++
	}

	kinds := make([]string, 0, len(coverage))
	for syntaxKind := range coverage {
		kinds = append(kinds, syntaxKind)
	}
	slices.Sort(kinds)
	report.BySyntaxKind = make([]KindCoverage, 0, len(kinds))
	for _, syntaxKind := range kinds {
		report.BySyntaxKind = append(report.BySyntaxKind, KindCoverage{SyntaxKind: syntaxKind, Metrics: *coverage[syntaxKind]})
	}
	return report, nil
}

func validateIdentity(file string, span semanticfacts.Span, syntaxKind string) error {
	if file == "" {
		return fmt.Errorf("file is required")
	}
	if span.Start < 0 || span.End < span.Start {
		return fmt.Errorf("invalid span [%d, %d)", span.Start, span.End)
	}
	if syntaxKind == "" {
		return fmt.Errorf("syntaxKind is required")
	}
	return nil
}

func validateNormalization(node Node, normalization Normalization) error {
	if err := validateIdentity(node.File, normalization.Span, normalization.SyntaxKind); err != nil {
		return err
	}
	sameSpan := node.Span == normalization.Span
	switch normalization.Rule {
	case NormalizationKindAlias:
		if !sameSpan {
			return fmt.Errorf("%s requires the canonical span", normalization.Rule)
		}
	case NormalizationProtocolInnerSpan:
		if sameSpan || normalization.Span.Start < node.Span.Start || normalization.Span.End > node.Span.End {
			return fmt.Errorf("%s requires a proper subspan of the canonical node span", normalization.Rule)
		}
	case NormalizationProtocolOuterSpan:
		if sameSpan || normalization.Span.Start > node.Span.Start || normalization.Span.End < node.Span.End {
			return fmt.Errorf("%s requires a proper enclosing span around the canonical node span", normalization.Rule)
		}
	default:
		return fmt.Errorf("unknown normalization rule %q", normalization.Rule)
	}
	return nil
}

func identityKey(file string, span semanticfacts.Span, syntaxKind string) key {
	return key{file: file, start: span.Start, end: span.End, syntaxKind: syntaxKind}
}

func exactCandidates(indexed map[NodeID]struct{}) []Candidate {
	result := make([]Candidate, 0, len(indexed))
	for nodeID := range indexed {
		result = append(result, Candidate{NodeID: nodeID})
	}
	slices.SortFunc(result, compareCandidates)
	return result
}

func normalizedCandidates(indexed map[NodeID]NormalizationRule) []Candidate {
	result := make([]Candidate, 0, len(indexed))
	for nodeID, rule := range indexed {
		result = append(result, Candidate{NodeID: nodeID, Rule: rule})
	}
	slices.SortFunc(result, compareCandidates)
	return result
}

func compareCandidates(left, right Candidate) int {
	if order := cmp.Compare(left.NodeID, right.NodeID); order != 0 {
		return order
	}
	return cmp.Compare(left.Rule, right.Rule)
}

func diagnosticFor(factIndex int, fact semanticfacts.FactRecord, code string, candidates []Candidate) Diagnostic {
	return Diagnostic{
		Code:       code,
		FactIndex:  factIndex,
		File:       fact.File,
		Span:       fact.Span,
		SyntaxKind: fact.SyntaxKind,
		Candidates: candidates,
	}
}

package semanticfacts

import "github.com/microsoft/typescript-go/internal/checker"

type graphInterners struct {
	types      *typeInterner
	symbols    *symbolInterner
	signatures *signatureInterner
}

func newGraphInterners(
	c *checker.Checker,
	files *fileRegistry,
	limits BudgetLimits,
	coreComposite bool,
	references bool,
	signatures bool,
) *graphInterners {
	types := newTypeInterner(c, limits, coreComposite)
	symbols := newSymbolInterner(c, files, references || signatures)
	signatureRecords := newSignatureInterner(c)
	types.symbols = symbols
	types.signatures = signatureRecords
	types.references = references || signatures
	types.signatureGraph = signatures
	symbols.types = types
	signatureRecords.types = types
	signatureRecords.symbols = symbols
	return &graphInterners{types: types, symbols: symbols, signatures: signatureRecords}
}

func (g *graphInterners) finalize(facts []FactRecord) {
	for changed := true; changed; {
		changed = false
		for index := range g.types.types {
			record := &g.types.types[index]
			if !record.Complete {
				continue
			}
			if !g.allTypesComplete(appendTypeIDs(record.Members, record.Target, record.TypeArguments, record.Constraint, record.Default)) {
				markTypeIncomplete(record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteType})
				changed = true
			}
			if !g.allSymbolsComplete(appendSymbolIDs(record.Properties, record.Symbol)) {
				markTypeIncomplete(record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteSymbol})
				changed = true
			}
			if !g.allSignaturesComplete(appendSignatureIDs(record.CallSignatures, record.ConstructSignatures, record.IndexSignatures)) {
				markTypeIncomplete(record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteSignature})
				changed = true
			}
		}
		for index := range g.symbols.symbols {
			record := &g.symbols.symbols[index]
			if !record.Complete {
				continue
			}
			if record.AliasedSymbol != "" && !g.symbols.complete(record.AliasedSymbol) {
				markSymbolIncomplete(record, GraphIssueReferencedAlias)
				changed = true
			}
			if !g.allSymbolsComplete(record.Members) {
				markSymbolIncomplete(record, GraphIssueReferencedIncompleteSymbol)
				changed = true
			}
			if !g.allTypesComplete([]TypeID{record.Type, record.DeclaredType}) {
				markSymbolIncomplete(record, GraphIssueReferencedIncompleteType)
				changed = true
			}
		}
		for index := range g.signatures.signatures {
			record := &g.signatures.signatures[index]
			if !record.Complete {
				continue
			}
			if !g.allSignaturesComplete([]SignatureID{record.Target}) {
				markSignatureIncomplete(record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteSignature})
				changed = true
			}
			if !g.allTypesComplete(appendTypeIDs(record.TypeArguments, record.IndexKeyType, record.TypeParameters, record.ThisType, record.ReturnType)) {
				markSignatureIncomplete(record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteType})
				changed = true
			}
			if !g.allSymbolsComplete(record.Parameters) {
				markSignatureIncomplete(record, EntityStateTruncated, GraphIssue{Code: GraphIssueReferencedIncompleteSymbol})
				changed = true
			}
		}
	}

	for index := range facts {
		fact := &facts[index]
		fact.Truncated = g.types.truncated(fact.TypeAtLocation) ||
			g.types.truncated(fact.AnnotationType) ||
			g.types.truncated(fact.InferredType) ||
			g.types.truncated(fact.ContextualType) ||
			g.types.truncated(fact.WidenedType) ||
			g.types.truncated(fact.ApparentType) ||
			g.types.truncated(fact.DeclaredType) ||
			g.types.truncated(fact.NarrowedType) ||
			g.types.truncated(fact.ConstraintType) ||
			g.symbols.truncated(fact.Symbol)
		fact.Complete = !fact.Recovered &&
			g.types.complete(fact.TypeAtLocation) &&
			g.types.complete(fact.AnnotationType) &&
			g.types.complete(fact.InferredType) &&
			g.types.complete(fact.ContextualType) &&
			g.types.complete(fact.WidenedType) &&
			g.types.complete(fact.ApparentType) &&
			g.types.complete(fact.DeclaredType) &&
			g.types.complete(fact.NarrowedType) &&
			g.types.complete(fact.ConstraintType) &&
			g.symbols.complete(fact.Symbol)
	}
}

func (g *graphInterners) allTypesComplete(ids []TypeID) bool {
	for _, id := range ids {
		if !g.types.complete(id) {
			return false
		}
	}
	return true
}

func (g *graphInterners) allSymbolsComplete(ids []SymbolID) bool {
	for _, id := range ids {
		if !g.symbols.complete(id) {
			return false
		}
	}
	return true
}

func (g *graphInterners) allSignaturesComplete(ids []SignatureID) bool {
	for _, id := range ids {
		if !g.signatures.complete(id) {
			return false
		}
	}
	return true
}

func appendSignatureIDs(groups ...[]SignatureID) []SignatureID {
	var result []SignatureID
	for _, group := range groups {
		result = append(result, group...)
	}
	return result
}

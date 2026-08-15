package semanticfacts

import (
	"fmt"

	"github.com/microsoft/typescript-go/internal/checker"
)

type signatureIdentity struct {
	signature *checker.Signature
	kind      string
}

type signatureInterner struct {
	checker     *checker.Checker
	types       *typeInterner
	symbols     *symbolInterner
	bySignature map[signatureIdentity]SignatureID
	byIndex     map[*checker.IndexInfo]SignatureID
	byID        map[SignatureID]int
	signatures  []SignatureRecord
}

func newSignatureInterner(c *checker.Checker) *signatureInterner {
	return &signatureInterner{
		checker:     c,
		bySignature: make(map[signatureIdentity]SignatureID),
		byIndex:     make(map[*checker.IndexInfo]SignatureID),
		byID:        make(map[SignatureID]int),
	}
}

func (i *signatureInterner) intern(signature *checker.Signature, kind string, depth int) SignatureID {
	if signature == nil {
		return ""
	}
	identity := signatureIdentity{signature: signature, kind: kind}
	if id, ok := i.bySignature[identity]; ok {
		return id
	}

	id, index := i.allocate(kind)
	i.bySignature[identity] = id
	record := i.signatures[index]
	if declaration := signature.Declaration(); declaration != nil {
		var ok bool
		record.Declaration, ok = i.symbols.internDeclarationNode(declaration)
		if !ok {
			markSignatureIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueUnrepresentableDecl})
		}
	}
	if target := signature.Target(); target != nil {
		if target == signature {
			record.Target = id
		} else {
			record.Target = i.intern(target, kind, depth)
		}
	}
	record.TypeArguments, _ = i.types.internMembers(i.checker.GetTypeArgumentsOfSignature(signature), depth)
	record.TypeParameters, _ = i.types.internMembers(signature.TypeParameters(), depth)
	if thisParameter := signature.ThisParameter(); thisParameter != nil {
		record.ThisType = i.types.internAtDepth(i.checker.GetTypeOfSymbol(thisParameter), depth)
	}
	for _, parameter := range signature.Parameters() {
		record.Parameters = append(record.Parameters, i.symbols.internAtDepth(parameter, depth))
	}
	record.MinArgumentCount = signature.MinArgumentCount()
	record.HasRestParameter = signature.HasRestParameter()
	record.ReturnType = i.types.internAtDepth(i.checker.GetReturnTypeOfSignature(signature), depth)
	if record.ReturnType == "" {
		markSignatureIncomplete(&record, EntityStateError, GraphIssue{Code: GraphIssueCheckerError})
	}
	i.signatures[index] = record
	return id
}

func (i *signatureInterner) internIndex(info *checker.IndexInfo, depth int) SignatureID {
	if info == nil {
		return ""
	}
	if id, ok := i.byIndex[info]; ok {
		return id
	}

	id, index := i.allocate("index")
	i.byIndex[info] = id
	record := i.signatures[index]
	if declaration := info.Declaration(); declaration != nil {
		var ok bool
		record.Declaration, ok = i.symbols.internDeclarationNode(declaration)
		if !ok {
			markSignatureIncomplete(&record, EntityStateTruncated, GraphIssue{Code: GraphIssueUnrepresentableDecl})
		}
	}
	record.IndexKeyType = i.types.internAtDepth(info.KeyType(), depth)
	record.MinArgumentCount = 1
	record.Readonly = info.IsReadonly()
	record.ReturnType = i.types.internAtDepth(info.ValueType(), depth)
	if record.IndexKeyType == "" || record.ReturnType == "" {
		markSignatureIncomplete(&record, EntityStateError, GraphIssue{Code: GraphIssueCheckerError})
	}
	i.signatures[index] = record
	return id
}

func (i *signatureInterner) allocate(kind string) (SignatureID, int) {
	id := SignatureID(fmt.Sprintf("signature:%d", len(i.signatures)+1))
	index := len(i.signatures)
	i.byID[id] = index
	i.signatures = append(i.signatures, SignatureRecord{
		Record:        "signature",
		ID:            id,
		SignatureKind: kind,
		State:         EntityStateComplete,
		Complete:      true,
	})
	return id, index
}

func (i *signatureInterner) complete(id SignatureID) bool {
	if id == "" {
		return true
	}
	index, ok := i.byID[id]
	return ok && i.signatures[index].Complete
}

func markSignatureIncomplete(record *SignatureRecord, state string, issue GraphIssue) {
	record.State = state
	record.Issues = appendGraphIssue(record.Issues, issue.Code)
	record.Complete = false
	record.Truncated = state == EntityStateTruncated
}

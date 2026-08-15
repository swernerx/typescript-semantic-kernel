package semanticfacts

import (
	"fmt"
	"sort"
	"strings"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/astnav"
	"github.com/microsoft/typescript-go/internal/checker"
	"github.com/microsoft/typescript-go/internal/compiler"
	"github.com/microsoft/typescript-go/internal/tspath"
	"github.com/microsoft/typescript-go/internal/vfs"
)

type fileRegistry struct {
	program     *compiler.Program
	projectRoot string
	fs          vfs.FS
	byPath      map[tspath.Path]*FileRecord
}

func newFileRegistry(program *compiler.Program, projectRoot string, fs vfs.FS) *fileRegistry {
	return &fileRegistry{
		program:     program,
		projectRoot: projectRoot,
		fs:          fs,
		byPath:      make(map[tspath.Path]*FileRecord),
	}
}

func (r *fileRegistry) addSelected(file *ast.SourceFile, id string, diagnosticCount int) {
	count := diagnosticCount
	r.byPath[file.Path()] = &FileRecord{
		Record:          "file",
		ID:              id,
		Origin:          "project",
		Selected:        true,
		DiagnosticCount: &count,
	}
}

func (r *fileRegistry) addDeclaration(file *ast.SourceFile) (string, bool) {
	if existing := r.byPath[file.Path()]; existing != nil {
		return existing.ID, true
	}

	id, origin, ok := r.identity(file)
	if !ok {
		return "", false
	}
	r.byPath[file.Path()] = &FileRecord{Record: "file", ID: id, Origin: origin}
	return id, true
}

func (r *fileRegistry) identity(file *ast.SourceFile) (string, string, bool) {
	if r.program.IsSourceFileDefaultLibrary(file.Path()) {
		return "typescript/lib/" + tspath.GetBaseFileName(file.FileName()), "typescript-lib", true
	}
	_, id, err := normalizeSourceIdentity(file.FileName(), r.projectRoot, r.fs)
	if err != nil {
		return "", "", false
	}
	return id, "project", true
}

func (r *fileRegistry) records() []FileRecord {
	records := make([]FileRecord, 0, len(r.byPath))
	for _, record := range r.byPath {
		records = append(records, *record)
	}
	sort.Slice(records, func(left, right int) bool { return records[left].ID < records[right].ID })
	return records
}

type declarationCandidate struct {
	node   *ast.Node
	record DeclarationRecord
}

type symbolInterner struct {
	checker       *checker.Checker
	files         *fileRegistry
	references    bool
	types         *typeInterner
	bySymbol      map[*ast.Symbol]SymbolID
	byID          map[SymbolID]int
	byDeclaration map[*ast.Node]DeclarationID
	symbols       []SymbolRecord
	declarations  []DeclarationRecord
}

func newSymbolInterner(c *checker.Checker, files *fileRegistry, references bool) *symbolInterner {
	return &symbolInterner{
		checker:       c,
		files:         files,
		references:    references,
		bySymbol:      make(map[*ast.Symbol]SymbolID),
		byID:          make(map[SymbolID]int),
		byDeclaration: make(map[*ast.Node]DeclarationID),
	}
}

func (i *symbolInterner) intern(symbol *ast.Symbol) SymbolID {
	return i.internAtDepth(symbol, 0)
}

func (i *symbolInterner) internAtDepth(symbol *ast.Symbol, depth int) SymbolID {
	if symbol == nil || symbol == i.checker.GetUnknownSymbol() {
		return ""
	}
	if id, ok := i.bySymbol[symbol]; ok {
		return id
	}

	id := SymbolID(fmt.Sprintf("symbol:%d", len(i.symbols)+1))
	i.bySymbol[symbol] = id
	i.byID[id] = len(i.symbols)
	i.symbols = append(i.symbols, SymbolRecord{
		Record:   "symbol",
		ID:       id,
		State:    EntityStateComplete,
		Complete: true,
	})
	index := len(i.symbols) - 1

	candidates := make([]declarationCandidate, 0, len(symbol.Declarations))
	complete := true
	issues := make([]GraphIssue, 0, 2)
	for _, declaration := range symbol.Declarations {
		candidate, ok := i.declarationCandidate(declaration)
		if !ok {
			complete = false
			issues = appendGraphIssue(issues, GraphIssueUnrepresentableDecl)
			continue
		}
		candidates = append(candidates, candidate)
	}
	sort.Slice(candidates, func(left, right int) bool {
		leftRecord := candidates[left].record
		rightRecord := candidates[right].record
		if leftRecord.File != rightRecord.File {
			return leftRecord.File < rightRecord.File
		}
		if leftRecord.Span.Start != rightRecord.Span.Start {
			return leftRecord.Span.Start < rightRecord.Span.Start
		}
		if leftRecord.Span.End != rightRecord.Span.End {
			return leftRecord.Span.End < rightRecord.Span.End
		}
		return leftRecord.SyntaxKind < rightRecord.SyntaxKind
	})

	declarations := make([]DeclarationID, 0, len(candidates))
	for _, candidate := range candidates {
		declarations = append(declarations, i.internDeclaration(candidate))
	}

	var aliasedSymbol SymbolID
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		aliased := i.checker.GetAliasedSymbol(symbol)
		if aliased == nil || aliased == symbol || aliased == i.checker.GetUnknownSymbol() {
			complete = false
			issues = appendGraphIssue(issues, GraphIssueUnresolvedAlias)
		} else {
			aliasedSymbol = i.internAtDepth(aliased, depth)
			if !i.complete(aliasedSymbol) {
				complete = false
				issues = appendGraphIssue(issues, GraphIssueReferencedAlias)
			}
		}
	}

	var symbolType TypeID
	var declaredType TypeID
	var members []SymbolID
	if i.references && symbol.Flags&ast.SymbolFlagsAlias == 0 {
		if symbol.Flags&ast.SymbolFlagsValue != 0 {
			symbolType = i.types.internAtDepth(i.checker.GetTypeOfSymbol(symbol), depth)
			if !i.types.complete(symbolType) {
				complete = false
				issues = appendGraphIssue(issues, GraphIssueReferencedIncompleteType)
			}
		}
		if symbol.Flags&ast.SymbolFlagsType != 0 {
			declaredType = i.types.internAtDepth(i.checker.GetDeclaredTypeOfSymbol(symbol), depth)
			if !i.types.complete(declaredType) {
				complete = false
				issues = appendGraphIssue(issues, GraphIssueReferencedIncompleteType)
			}
		}
		memberSymbols := make([]*ast.Symbol, 0, len(symbol.Members))
		for name, member := range symbol.Members {
			if !strings.HasPrefix(name, ast.InternalSymbolNamePrefix) {
				memberSymbols = append(memberSymbols, member)
			}
		}
		sort.SliceStable(memberSymbols, func(left, right int) bool {
			leftName := ast.EscapeSymbolName(ast.SymbolName(memberSymbols[left]))
			rightName := ast.EscapeSymbolName(ast.SymbolName(memberSymbols[right]))
			return leftName < rightName
		})
		for _, member := range memberSymbols {
			memberID := i.internAtDepth(member, depth)
			members = append(members, memberID)
			if !i.complete(memberID) {
				complete = false
				issues = appendGraphIssue(issues, GraphIssueReferencedIncompleteSymbol)
			}
		}
	}
	state := EntityStateComplete
	if !complete {
		state = EntityStateTruncated
	}

	i.symbols[index] = SymbolRecord{
		Record:        "symbol",
		ID:            id,
		Name:          ast.EscapeSymbolName(ast.SymbolName(symbol)),
		Roles:         symbolRoles(symbol.Flags),
		Declarations:  declarations,
		AliasedSymbol: aliasedSymbol,
		Type:          symbolType,
		DeclaredType:  declaredType,
		Members:       members,
		State:         state,
		Issues:        issues,
		Complete:      complete,
		Truncated:     !complete,
	}
	return id
}

func markSymbolIncomplete(record *SymbolRecord, code string) {
	record.State = EntityStateTruncated
	record.Issues = appendGraphIssue(record.Issues, code)
	record.Complete = false
	record.Truncated = true
}

func appendGraphIssue(issues []GraphIssue, code string) []GraphIssue {
	for _, issue := range issues {
		if issue.Code == code {
			return issues
		}
	}
	issues = append(issues, GraphIssue{Code: code})
	sort.Slice(issues, func(left, right int) bool { return issues[left].Code < issues[right].Code })
	return issues
}

func (i *symbolInterner) declarationCandidate(declaration *ast.Node) (declarationCandidate, bool) {
	if declaration == nil {
		return declarationCandidate{}, false
	}
	file := ast.GetSourceFileOfNode(declaration)
	if file == nil {
		return declarationCandidate{}, false
	}
	fileID, ok := i.files.addDeclaration(file)
	if !ok {
		return declarationCandidate{}, false
	}
	location := declaration.Name()
	if location == nil {
		location = declaration
	}
	return declarationCandidate{
		node: declaration,
		record: DeclarationRecord{
			Record:     "declaration",
			File:       fileID,
			Span:       Span{Start: astnav.GetStartOfNode(location, file, false), End: location.End()},
			SyntaxKind: declaration.Kind.String(),
		},
	}, true
}

func (i *symbolInterner) internDeclaration(candidate declarationCandidate) DeclarationID {
	if id, ok := i.byDeclaration[candidate.node]; ok {
		return id
	}
	id := DeclarationID(fmt.Sprintf("declaration:%d", len(i.declarations)+1))
	i.byDeclaration[candidate.node] = id
	candidate.record.ID = id
	i.declarations = append(i.declarations, candidate.record)
	return id
}

func (i *symbolInterner) internDeclarationNode(node *ast.Node) (DeclarationID, bool) {
	candidate, ok := i.declarationCandidate(node)
	if !ok {
		return "", false
	}
	return i.internDeclaration(candidate), true
}

func (i *symbolInterner) complete(id SymbolID) bool {
	if id == "" {
		return true
	}
	index, ok := i.byID[id]
	return ok && i.symbols[index].Complete
}

func (i *symbolInterner) truncated(id SymbolID) bool {
	if id == "" {
		return false
	}
	index, ok := i.byID[id]
	return ok && i.symbols[index].State == EntityStateTruncated
}

func (i *symbolInterner) declarationsOf(id SymbolID) []DeclarationID {
	index, ok := i.byID[id]
	if !ok {
		return nil
	}
	return i.symbols[index].Declarations
}

func symbolRoles(flags ast.SymbolFlags) []string {
	roles := make([]string, 0, 4)
	for _, role := range []struct {
		flag ast.SymbolFlags
		name string
	}{
		{ast.SymbolFlagsAlias, "alias"},
		{ast.SymbolFlagsVariable, "variable"},
		{ast.SymbolFlagsProperty, "property"},
		{ast.SymbolFlagsEnumMember, "enum_member"},
		{ast.SymbolFlagsFunction, "function"},
		{ast.SymbolFlagsClass, "class"},
		{ast.SymbolFlagsInterface, "interface"},
		{ast.SymbolFlagsEnum, "enum"},
		{ast.SymbolFlagsModule, "module"},
		{ast.SymbolFlagsMethod, "method"},
		{ast.SymbolFlagsConstructor, "constructor"},
		{ast.SymbolFlagsAccessor, "accessor"},
		{ast.SymbolFlagsSignature, "signature"},
		{ast.SymbolFlagsTypeParameter, "type_parameter"},
		{ast.SymbolFlagsTypeAlias, "type_alias"},
		{ast.SymbolFlagsOptional, "optional"},
		{ast.SymbolFlagsTransient, "transient"},
	} {
		if flags&role.flag != 0 {
			roles = append(roles, role.name)
		}
	}
	if len(roles) == 0 {
		return []string{"unknown"}
	}
	return roles
}

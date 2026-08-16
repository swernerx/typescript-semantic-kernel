package semanticfacts

import (
	"context"
	"errors"
	"fmt"
	"slices"
	"sort"
	"strings"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/astnav"
	"github.com/microsoft/typescript-go/internal/checker"
	"github.com/microsoft/typescript-go/internal/compiler"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/tsoptions"
	"github.com/microsoft/typescript-go/internal/tspath"
	"github.com/microsoft/typescript-go/internal/vfs"
)

type AnalyzerOptions struct {
	CurrentDirectory   string
	FS                 vfs.FS
	DefaultLibraryPath string
}

type selectedFile struct {
	file            *ast.SourceFile
	id              string
	diagnosticCount int
}

func Analyze(ctx context.Context, options AnalyzerOptions, request Request) (*Result, error) {
	limits, err := prepareAnalysis(ctx, request)
	if err != nil {
		return nil, err
	}
	if options.FS == nil {
		return nil, errors.New("filesystem is required")
	}

	currentDirectory := tspath.NormalizePath(options.CurrentDirectory)
	if !tspath.IsRootedDiskPath(currentDirectory) {
		return nil, fmt.Errorf("current directory %q must be an absolute disk path", options.CurrentDirectory)
	}
	projectPath := tspath.GetNormalizedAbsolutePath(request.Project, currentDirectory)
	host := compiler.NewCompilerHost(currentDirectory, options.FS, options.DefaultLibraryPath, nil, nil)
	parsed, configErrors := tsoptions.GetParsedCommandLineOfConfigFile(projectPath, &core.CompilerOptions{}, nil, host, nil)
	if len(configErrors) != 0 {
		return nil, fmt.Errorf("project configuration %q contains %d error(s)", request.Project, len(configErrors))
	}

	program := compiler.NewProgram(compiler.ProgramOptions{Config: parsed, Host: host})
	program.BindSourceFiles()
	return analyzeProgram(ctx, program, projectPath, parsed.CompilerOptions(), options.FS, limits, request)
}

// AnalyzeProgram builds semantic facts from an existing compiler program. The
// caller retains ownership of the program and checker lifecycle. API sessions
// use this entry point so facts observe the exact pinned project snapshot,
// including unsaved overlays and temporary file updates.
func AnalyzeProgram(ctx context.Context, program *compiler.Program, request Request) (*Result, error) {
	limits, err := prepareAnalysis(ctx, request)
	if err != nil {
		return nil, err
	}
	if program == nil {
		return nil, errors.New("program is required")
	}
	currentDirectory := tspath.NormalizePath(program.GetCurrentDirectory())
	if !tspath.IsRootedDiskPath(currentDirectory) {
		return nil, fmt.Errorf("current directory %q must be an absolute disk path", program.GetCurrentDirectory())
	}
	projectPath := tspath.GetNormalizedAbsolutePath(request.Project, currentDirectory)
	return analyzeProgram(ctx, program, projectPath, program.Options(), program.Host().FS(), limits, request)
}

func prepareAnalysis(ctx context.Context, request Request) (BudgetLimits, error) {
	if err := ctx.Err(); err != nil {
		return BudgetLimits{}, err
	}
	if err := validateRequest(request); err != nil {
		return BudgetLimits{}, err
	}
	return normalizeBudgetLimits(request.Budgets)
}

func analyzeProgram(
	ctx context.Context,
	program *compiler.Program,
	projectPath string,
	compilerOptions any,
	fs vfs.FS,
	limits BudgetLimits,
	request Request,
) (*Result, error) {
	currentDirectory := tspath.NormalizePath(program.GetCurrentDirectory())
	projectRoot := tspath.GetDirectoryPath(projectPath)

	allowedFiles, allowedFilesErr := normalizeAllowedFiles(request.Files, projectRoot, fs)
	if allowedFilesErr != nil {
		return nil, allowedFilesErr
	}
	selected := make(map[tspath.Path]*selectedFile)
	resolvedSelections := make([]struct {
		selection Selection
		selected  *selectedFile
	}, 0, len(request.Selections))
	resolveFile := func(fileName string) (*selectedFile, error) {
		absolute, _, identityErr := normalizeSourceIdentity(fileName, projectRoot, fs)
		if identityErr != nil {
			return nil, identityErr
		}
		path := tspath.ToPath(absolute, projectRoot, fs.UseCaseSensitiveFileNames())
		if entry := selected[path]; entry != nil {
			return entry, nil
		}
		file := program.GetSourceFile(absolute)
		if file == nil {
			return nil, fmt.Errorf("source file %q is not part of project %q", fileName, request.Project)
		}
		_, id, canonicalIdentityErr := normalizeSourceIdentity(file.FileName(), projectRoot, fs)
		if canonicalIdentityErr != nil {
			return nil, canonicalIdentityErr
		}
		entry := &selectedFile{file: file, id: id}
		selected[file.Path()] = entry
		return entry, nil
	}

	if len(request.Selections) == 0 {
		for _, fileName := range request.Files {
			if _, resolveErr := resolveFile(fileName); resolveErr != nil {
				return nil, resolveErr
			}
		}
	} else {
		for _, selection := range request.Selections {
			absolute, _, identityErr := normalizeSourceIdentity(selection.File, projectRoot, fs)
			if identityErr != nil {
				return nil, identityErr
			}
			path := tspath.ToPath(absolute, projectRoot, fs.UseCaseSensitiveFileNames())
			if len(allowedFiles) != 0 {
				if _, ok := allowedFiles[path]; !ok {
					return nil, fmt.Errorf("selection file %q is not present in files", selection.File)
				}
			}
			entry, resolveErr := resolveFile(selection.File)
			if resolveErr != nil {
				return nil, resolveErr
			}
			resolvedSelections = append(resolvedSelections, struct {
				selection Selection
				selected  *selectedFile
			}{selection: selection, selected: entry})
		}
	}

	diagnosticCount := 0
	files := newFileRegistry(program, projectRoot, fs)
	for _, entry := range selected {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		entry.diagnosticCount = len(program.GetSyntacticDiagnostics(ctx, entry.file)) + len(program.GetSemanticDiagnostics(ctx, entry.file))
		diagnosticCount += entry.diagnosticCount
		files.addSelected(entry.file, entry.id, entry.diagnosticCount)
	}

	c, done := program.GetTypeChecker(ctx)
	defer done()
	if len(request.Selections) == 0 {
		fileWideEntries := make([]*selectedFile, 0, len(selected))
		for _, entry := range selected {
			fileWideEntries = append(fileWideEntries, entry)
		}
		sort.Slice(fileWideEntries, func(left, right int) bool { return fileWideEntries[left].id < fileWideEntries[right].id })
		for _, entry := range fileWideEntries {
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			for _, selection := range enumerateSemanticSelections(c, entry) {
				resolvedSelections = append(resolvedSelections, struct {
					selection Selection
					selected  *selectedFile
				}{selection: selection, selected: entry})
			}
		}
	}
	graph := newGraphInterners(
		c,
		files,
		limits,
		slices.Contains(request.RequiredCapabilities, CapabilityCoreCompositeTypes),
		slices.Contains(request.RequiredCapabilities, CapabilityAdvancedTypes),
		slices.Contains(request.RequiredCapabilities, CapabilityGraphReferences),
		slices.Contains(request.RequiredCapabilities, CapabilityGraphSignatures),
	)
	facts := make([]FactRecord, 0, len(resolvedSelections))
	for _, resolved := range resolvedSelections {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		fact, factErr := analyzeSelection(c, graph.types, graph.symbols, resolved.selected, resolved.selection)
		if factErr != nil {
			return nil, factErr
		}
		facts = append(facts, fact)
	}
	graph.finalize(facts)

	projectID, projectIdentityErr := projectIdentity(projectPath, currentDirectory, fs)
	if projectIdentityErr != nil {
		return nil, projectIdentityErr
	}
	return &Result{
		Header: HeaderRecord{
			Record:             "header",
			SchemaVersion:      SchemaVersion,
			TypeScriptVersion:  core.Version(),
			TypeScriptRevision: UpstreamRevision,
			OffsetEncoding:     OffsetEncoding,
			Capabilities:       SupportedCapabilities(),
			Budgets:            graph.types.budgetReport(),
			Project:            projectID,
			CompilerOptions:    compilerOptions,
			DiagnosticCount:    diagnosticCount,
		},
		Files:        files.records(),
		Types:        graph.types.types,
		Declarations: graph.symbols.declarations,
		Symbols:      graph.symbols.symbols,
		Signatures:   graph.signatures.signatures,
		Facts:        facts,
	}, nil
}

func validateRequest(request Request) error {
	if request.SchemaVersion != SchemaVersion {
		return fmt.Errorf("unsupported schemaVersion %d; expected %d", request.SchemaVersion, SchemaVersion)
	}
	if request.Project == "" {
		return errors.New("project is required")
	}
	if err := validateRequiredCapabilities(request.RequiredCapabilities); err != nil {
		return err
	}
	if _, err := normalizeBudgetLimits(request.Budgets); err != nil {
		return err
	}
	if len(request.Selections) == 0 && len(request.Files) == 0 {
		return errors.New("at least one file or selection is required")
	}
	for index, selection := range request.Selections {
		if selection.File == "" {
			return fmt.Errorf("selections[%d].file is required", index)
		}
		if selection.Start < 0 || selection.End < selection.Start {
			return fmt.Errorf("selections[%d] has invalid span [%d, %d)", index, selection.Start, selection.End)
		}
	}
	return nil
}

func enumerateSemanticSelections(c *checker.Checker, selected *selectedFile) []Selection {
	type candidate struct {
		start int
		end   int
		kind  ast.Kind
	}
	candidates := make([]candidate, 0, selected.file.IdentifierCount)
	var visit func(*ast.Node)
	visit = func(node *ast.Node) {
		if node == nil || node.Flags&ast.NodeFlagsReparsed != 0 {
			return
		}
		if isSemanticOccurrenceToken(node.Kind) && c.GetTypeAtLocation(node) != nil {
			start := astnav.GetStartOfNode(node, selected.file, false)
			if start < node.End() {
				candidates = append(candidates, candidate{start: start, end: node.End(), kind: node.Kind})
			}
		}
		for child := range node.IterChildren() {
			visit(child)
		}
	}
	visit(selected.file.AsNode())
	sort.Slice(candidates, func(left, right int) bool {
		if candidates[left].start != candidates[right].start {
			return candidates[left].start < candidates[right].start
		}
		if candidates[left].end != candidates[right].end {
			return candidates[left].end < candidates[right].end
		}
		return candidates[left].kind < candidates[right].kind
	})
	selections := make([]Selection, 0, len(candidates))
	for index, occurrence := range candidates {
		if index != 0 && occurrence == candidates[index-1] {
			continue
		}
		selections = append(selections, Selection{File: selected.id, Start: occurrence.start, End: occurrence.end})
	}
	return selections
}

func isSemanticOccurrenceToken(kind ast.Kind) bool {
	return kind == ast.KindIdentifier ||
		kind == ast.KindPrivateIdentifier ||
		ast.IsLiteralKind(kind) ||
		ast.IsPseudoLiteralKind(kind) ||
		ast.IsKeywordExpressionKind(kind) ||
		ast.IsKeywordTypeKind(kind)
}

func normalizeAllowedFiles(files []string, projectRoot string, fs vfs.FS) (map[tspath.Path]struct{}, error) {
	if len(files) == 0 {
		return nil, nil
	}
	result := make(map[tspath.Path]struct{}, len(files))
	for _, file := range files {
		absolute, _, err := normalizeSourceIdentity(file, projectRoot, fs)
		if err != nil {
			return nil, err
		}
		result[tspath.ToPath(absolute, projectRoot, fs.UseCaseSensitiveFileNames())] = struct{}{}
	}
	return result, nil
}

func normalizeSourceIdentity(fileName string, projectRoot string, fs vfs.FS) (string, string, error) {
	absolute := tspath.GetNormalizedAbsolutePath(fileName, projectRoot)
	compareOptions := tspath.ComparePathsOptions{
		CurrentDirectory:          projectRoot,
		UseCaseSensitiveFileNames: fs.UseCaseSensitiveFileNames(),
	}
	if !tspath.ContainsPath(projectRoot, absolute, compareOptions) {
		return "", "", fmt.Errorf("source file %q is outside project root", fileName)
	}
	id := tspath.GetRelativePathFromDirectory(projectRoot, absolute, compareOptions)
	if id == "" || id == "." || strings.HasPrefix(id, "../") {
		return "", "", fmt.Errorf("source file %q does not identify a project file", fileName)
	}
	return absolute, tspath.NormalizeSlashes(id), nil
}

func projectIdentity(projectPath string, currentDirectory string, fs vfs.FS) (string, error) {
	compareOptions := tspath.ComparePathsOptions{
		CurrentDirectory:          currentDirectory,
		UseCaseSensitiveFileNames: fs.UseCaseSensitiveFileNames(),
	}
	id := tspath.GetRelativePathFromDirectory(currentDirectory, projectPath, compareOptions)
	if id == "" {
		return "", errors.New("project path cannot resolve to the current directory")
	}
	return tspath.NormalizeSlashes(id), nil
}

func analyzeSelection(c *checker.Checker, types *typeInterner, symbols *symbolInterner, selected *selectedFile, selection Selection) (FactRecord, error) {
	textLength := len(selected.file.Text())
	if selection.End > textLength {
		return FactRecord{}, fmt.Errorf("selection [%d, %d) exceeds %q length %d", selection.Start, selection.End, selection.File, textLength)
	}
	node := astnav.GetTouchingToken(selected.file, selection.Start)
	if node == nil || node.Kind == ast.KindEndOfFile {
		return FactRecord{}, fmt.Errorf("selection [%d, %d) in %q does not identify a source token", selection.Start, selection.End, selection.File)
	}
	start := astnav.GetStartOfNode(node, selected.file, false)
	if selection.Start < start || selection.End > node.End() || selection.Start == selection.End && selection.Start == node.End() {
		return FactRecord{}, fmt.Errorf("selection [%d, %d) in %q must fit inside one token", selection.Start, selection.End, selection.File)
	}

	observed := c.GetTypeAtLocation(node)
	if observed == nil {
		return FactRecord{}, fmt.Errorf("selection [%d, %d) in %q has no semantic type", selection.Start, selection.End, selection.File)
	}
	fact := FactRecord{
		Record:     "fact",
		File:       selected.id,
		Span:       Span{Start: start, End: node.End()},
		SyntaxKind: node.Kind.String(),
		Recovered:  selected.diagnosticCount != 0,
	}
	fact.ActualType = types.intern(observed)
	fact.TypeAtLocation = fact.ActualType
	fact.TypeViewStates.Actual = TypeViewAvailable
	symbol := c.GetSymbolAtLocation(node)
	fact.Symbol = symbols.intern(symbol)
	fact.Declarations = symbols.declarationsOf(fact.Symbol)
	views := classifyTypeViews(c, node, symbol, observed)
	fact.AnnotationType = types.intern(views.annotation)
	fact.InferredType = types.intern(views.inferred)
	fact.NarrowedType = types.intern(views.narrowed)
	fact.ContextualType, fact.TypeViewStates.Contextual = internOptionalTypeView(
		types,
		observed,
		contextualTypeAtOccurrence(c, node),
		ast.IsExpressionNode(node),
	)
	fact.WidenedType, fact.TypeViewStates.Widened = internOptionalTypeView(types, observed, c.GetWidenedType(observed), true)
	fact.ApparentType, fact.TypeViewStates.Apparent = internOptionalTypeView(types, observed, c.GetApparentType(observed), true)
	declared, declaredApplies := declaredTypeAtOccurrence(c, node, symbol)
	fact.DeclaredType, fact.TypeViewStates.Declared = internOptionalTypeView(types, observed, declared, declaredApplies)
	if constraint := c.GetBaseConstraintOfType(observed); constraint != nil && constraint != observed {
		fact.ConstraintType = types.intern(constraint)
	}

	return fact, nil
}

func contextualTypeAtOccurrence(c *checker.Checker, node *ast.Node) *checker.Type {
	if !ast.IsExpressionNode(node) {
		return nil
	}
	return c.GetContextualType(node, checker.ContextFlagsNone)
}

func declaredTypeAtOccurrence(c *checker.Checker, node *ast.Node, symbol *ast.Symbol) (*checker.Type, bool) {
	if symbol == nil {
		return nil, false
	}
	valueOccurrence := isValueOccurrence(c, node, symbol)
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = c.GetAliasedSymbol(symbol)
		if symbol == nil {
			return nil, true
		}
	}
	if valueOccurrence && symbol.Flags&ast.SymbolFlagsValue != 0 {
		return c.GetTypeOfSymbolAtLocation(symbol, nil), true
	}
	if symbol.Flags&ast.SymbolFlagsType != 0 {
		return c.GetDeclaredTypeOfSymbol(symbol), true
	}
	return nil, false
}

func internOptionalTypeView(types *typeInterner, actual *checker.Type, candidate *checker.Type, applies bool) (TypeID, string) {
	if !applies {
		return "", TypeViewInapplicable
	}
	if candidate == nil {
		return "", TypeViewUnavailable
	}
	if candidate == actual {
		return "", TypeViewSameAsActual
	}
	return types.intern(candidate), TypeViewAvailable
}

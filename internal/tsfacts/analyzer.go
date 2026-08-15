package tsfacts

import (
	"context"
	"errors"
	"fmt"
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
	if err := validateRequest(request); err != nil {
		return nil, err
	}
	limits, err := normalizeBudgetLimits(request.Budgets)
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
	projectRoot := tspath.GetDirectoryPath(projectPath)
	host := compiler.NewCompilerHost(currentDirectory, options.FS, options.DefaultLibraryPath, nil, nil)
	parsed, configErrors := tsoptions.GetParsedCommandLineOfConfigFile(projectPath, &core.CompilerOptions{}, nil, host, nil)
	if len(configErrors) != 0 {
		return nil, fmt.Errorf("project configuration %q contains %d error(s)", request.Project, len(configErrors))
	}

	program := compiler.NewProgram(compiler.ProgramOptions{Config: parsed, Host: host})
	program.BindSourceFiles()

	allowedFiles, allowedFilesErr := normalizeAllowedFiles(request.Files, projectRoot, options.FS)
	if allowedFilesErr != nil {
		return nil, allowedFilesErr
	}
	selected := make(map[tspath.Path]*selectedFile)
	resolvedSelections := make([]struct {
		selection Selection
		selected  *selectedFile
	}, 0, len(request.Selections))

	for _, selection := range request.Selections {
		absolute, _, identityErr := normalizeSourceIdentity(selection.File, projectRoot, options.FS)
		if identityErr != nil {
			return nil, identityErr
		}
		path := tspath.ToPath(absolute, projectRoot, options.FS.UseCaseSensitiveFileNames())
		if len(allowedFiles) != 0 {
			if _, ok := allowedFiles[path]; !ok {
				return nil, fmt.Errorf("selection file %q is not present in files", selection.File)
			}
		}
		file := program.GetSourceFile(absolute)
		if file == nil {
			return nil, fmt.Errorf("selection file %q is not part of project %q", selection.File, request.Project)
		}
		entry := selected[file.Path()]
		if entry == nil {
			_, id, canonicalIdentityErr := normalizeSourceIdentity(file.FileName(), projectRoot, options.FS)
			if canonicalIdentityErr != nil {
				return nil, canonicalIdentityErr
			}
			entry = &selectedFile{file: file, id: id}
			selected[file.Path()] = entry
		}
		resolvedSelections = append(resolvedSelections, struct {
			selection Selection
			selected  *selectedFile
		}{selection: selection, selected: entry})
	}

	diagnosticCount := 0
	files := newFileRegistry(program, projectRoot, options.FS)
	for _, entry := range selected {
		entry.diagnosticCount = len(program.GetSyntacticDiagnostics(ctx, entry.file)) + len(program.GetSemanticDiagnostics(ctx, entry.file))
		diagnosticCount += entry.diagnosticCount
		files.addSelected(entry.file, entry.id, entry.diagnosticCount)
	}

	c, done := program.GetTypeChecker(ctx)
	defer done()
	types := newTypeInterner(c, limits)
	symbols := newSymbolInterner(c, files)
	facts := make([]FactRecord, 0, len(resolvedSelections))
	for _, resolved := range resolvedSelections {
		fact, factErr := analyzeSelection(c, types, symbols, resolved.selected, resolved.selection)
		if factErr != nil {
			return nil, factErr
		}
		facts = append(facts, fact)
	}

	projectID, projectIdentityErr := projectIdentity(projectPath, currentDirectory, options.FS)
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
			Budgets:            types.budgetReport(),
			Project:            projectID,
			CompilerOptions:    parsed.CompilerOptions(),
			DiagnosticCount:    diagnosticCount,
		},
		Files:        files.records(),
		Types:        types.types,
		Declarations: symbols.declarations,
		Symbols:      symbols.symbols,
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
	if len(request.Selections) == 0 {
		return errors.New("at least one selection is required")
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

	fact.Truncated = types.truncated(fact.TypeAtLocation) ||
		types.truncated(fact.AnnotationType) ||
		types.truncated(fact.InferredType) ||
		types.truncated(fact.ContextualType) ||
		types.truncated(fact.WidenedType) ||
		types.truncated(fact.ApparentType) ||
		types.truncated(fact.DeclaredType) ||
		types.truncated(fact.NarrowedType) ||
		types.truncated(fact.ConstraintType) ||
		symbols.truncated(fact.Symbol)
	fact.Complete = !fact.Recovered &&
		types.complete(fact.TypeAtLocation) &&
		types.complete(fact.AnnotationType) &&
		types.complete(fact.InferredType) &&
		types.complete(fact.ContextualType) &&
		types.complete(fact.WidenedType) &&
		types.complete(fact.ApparentType) &&
		types.complete(fact.DeclaredType) &&
		types.complete(fact.NarrowedType) &&
		types.complete(fact.ConstraintType) &&
		symbols.complete(fact.Symbol)
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

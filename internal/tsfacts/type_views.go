package tsfacts

import (
	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/checker"
)

type typeViews struct {
	annotation *checker.Type
	inferred   *checker.Type
	narrowed   *checker.Type
}

func classifyTypeViews(c *checker.Checker, node *ast.Node, symbol *ast.Symbol, observed *checker.Type) typeViews {
	if !isValueOccurrence(c, node, symbol) {
		return typeViews{}
	}

	baseline := c.GetTypeOfSymbolAtLocation(symbol, nil)
	views := typeViews{}
	if annotation := directTypeAnnotation(c, symbol); annotation != nil {
		views.annotation = annotation
	} else {
		views.inferred = baseline
	}
	if isNarrowedAtOccurrence(c, node, observed, baseline) {
		views.narrowed = observed
	}
	return views
}

func isNarrowedAtOccurrence(c *checker.Checker, node *ast.Node, observed *checker.Type, baseline *checker.Type) bool {
	if baseline != nil && observed != baseline {
		return true
	}
	if node.Parent == nil || node.Parent.Kind != ast.KindPropertyAccessExpression ||
		node.Parent.AsPropertyAccessExpression().Name() != node {
		return false
	}

	receiver := node.Parent.Expression()
	receiverSymbol := c.GetSymbolAtLocation(receiver)
	if receiverSymbol == nil {
		return false
	}
	receiverObserved := c.GetTypeAtLocation(receiver)
	receiverBaseline := c.GetTypeOfSymbolAtLocation(receiverSymbol, nil)
	return isNarrowedAtOccurrence(c, receiver, receiverObserved, receiverBaseline)
}

func isValueOccurrence(c *checker.Checker, node *ast.Node, symbol *ast.Symbol) bool {
	if node == nil || symbol == nil || symbol.Flags&(ast.SymbolFlagsValue|ast.SymbolFlagsAlias) == 0 {
		return false
	}
	if symbol.Flags&ast.SymbolFlagsAlias != 0 && c.GetTypeOnlyAliasDeclaration(symbol) != nil {
		return false
	}
	return ast.IsExpressionNode(node) || ast.IsDeclarationName(node)
}

func directTypeAnnotation(c *checker.Checker, symbol *ast.Symbol) *checker.Type {
	var annotation *checker.Type
	for _, declaration := range symbol.Declarations {
		if !supportsWholeSymbolAnnotation(declaration) {
			continue
		}
		name := declaration.Name()
		if name == nil || ast.IsBindingPattern(name) {
			continue
		}
		typeNode := ast.GetTypeAnnotationNode(declaration)
		if typeNode == nil {
			continue
		}
		candidate := c.GetTypeFromTypeNode(typeNode)
		if candidate == nil {
			continue
		}
		if annotation != nil && annotation != candidate {
			return nil
		}
		annotation = candidate
	}
	return annotation
}

func supportsWholeSymbolAnnotation(node *ast.Node) bool {
	return ast.IsVariableDeclaration(node) ||
		ast.IsParameterDeclaration(node) ||
		ast.IsPropertySignatureDeclaration(node) ||
		ast.IsPropertyDeclaration(node)
}

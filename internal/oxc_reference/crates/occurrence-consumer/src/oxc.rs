use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::{NodeId, Semantic, SemanticBuilder};
use oxc_span::{GetSpan, SourceType};

use crate::contract::{
    NodeCandidate, Normalization, NormalizationRule, Occurrence, Report, Span, correlate,
};
use crate::facts::{OccurrenceTypeFacts, SemanticSnapshot, TypeGraph};
use crate::inspector::{GraphInspector, InspectionReport, InspectorLimits};

/// An arena-scoped OXC view. Typed `NodeId`s never leave this consumer; the
/// portable report serializes their numeric form only at the JSON boundary.
pub struct OxcConsumer<'a> {
    file: String,
    source: &'a str,
    node_count: usize,
    candidates: Vec<NodeCandidate>,
    projected_node_ids: HashMap<u32, NodeId>,
    mapped_nodes: HashMap<usize, NodeId>,
    snapshot: Option<Arc<SemanticSnapshot>>,
    fact_indices_by_node: HashMap<NodeId, Vec<usize>>,
    mapping_report: Option<Report>,
}

impl<'a> OxcConsumer<'a> {
    pub fn parse(allocator: &'a Allocator, file: &str, source: &'a str) -> Result<Self, String> {
        let source_type = SourceType::from_path(Path::new(file))
            .map_err(|_| format!("unsupported source type for {file:?}"))?;
        let parsed = Parser::new(allocator, source, source_type)
            .with_options(ParseOptions {
                preserve_parens: true,
                ..ParseOptions::default()
            })
            .parse();
        if !parsed.diagnostics.is_empty() || parsed.panicked {
            return Err(format!(
                "OXC failed to parse {file:?}: {} diagnostic(s), panicked={}",
                parsed.diagnostics.len(),
                parsed.panicked
            ));
        }

        let built = SemanticBuilder::new_compiler()
            .with_build_nodes(true)
            .build(&parsed.program);
        if !built.diagnostics.is_empty() {
            return Err(format!(
                "OXC semantic analysis failed for {file:?}: {} diagnostic(s)",
                built.diagnostics.len()
            ));
        }
        let semantic = built.semantic;
        let node_count = semantic.nodes().len();
        let (candidates, projected_node_ids) = project_nodes(file, source, &semantic);
        Ok(Self {
            file: file.to_owned(),
            source,
            node_count,
            candidates,
            projected_node_ids,
            mapped_nodes: HashMap::new(),
            snapshot: None,
            fact_indices_by_node: HashMap::new(),
            mapping_report: None,
        })
    }

    pub fn correlate(&mut self, facts: &[Occurrence]) -> Result<Report, String> {
        if facts.iter().any(|fact| fact.file != self.file) {
            return Err(format!(
                "all facts must belong to consumer file {:?}",
                self.file
            ));
        }
        self.snapshot = None;
        self.fact_indices_by_node.clear();
        self.mapping_report = None;
        let report = correlate(facts, &self.candidates)?;
        self.mapped_nodes.clear();
        for mapping in &report.mappings {
            let node_id = self
                .projected_node_ids
                .get(&mapping.node_id)
                .ok_or_else(|| {
                    format!("mapping references unknown OXC NodeId {}", mapping.node_id)
                })?;
            self.mapped_nodes.insert(mapping.fact_index, *node_id);
        }
        Ok(report)
    }

    /// Correlates every semantic occurrence and retains response-local graph
    /// ownership exactly once. Repeated fact selections are preserved in fact
    /// order rather than overwriting an earlier attachment to the same node.
    pub fn attach(&mut self, snapshot: Arc<SemanticSnapshot>) -> Result<Report, String> {
        let occurrences = snapshot
            .facts()
            .iter()
            .map(OccurrenceTypeFacts::occurrence)
            .collect::<Vec<_>>();
        let report = self.correlate(&occurrences)?;
        self.fact_indices_by_node.clear();
        for mapping in &report.mappings {
            let node_id = self
                .mapped_nodes
                .get(&mapping.fact_index)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "mapping for fact {} lost its typed OXC NodeId",
                        mapping.fact_index
                    )
                })?;
            self.fact_indices_by_node
                .entry(node_id)
                .or_default()
                .push(mapping.fact_index);
        }
        self.snapshot = Some(snapshot);
        self.mapping_report = Some(report.clone());
        Ok(report)
    }

    pub fn node_for_fact(&self, fact_index: usize) -> Option<NodeId> {
        self.mapped_nodes.get(&fact_index).copied()
    }

    pub fn type_facts_for_node(
        &self,
        node_id: NodeId,
    ) -> impl Iterator<Item = AttachedTypeFacts<'_>> {
        self.fact_indices_by_node
            .get(&node_id)
            .into_iter()
            .flatten()
            .filter_map(|fact_index| {
                let snapshot = self.snapshot.as_deref()?;
                let facts = snapshot.facts().get(*fact_index)?;
                Some(AttachedTypeFacts {
                    fact_index: *fact_index,
                    facts,
                    graph: snapshot.graph(),
                })
            })
    }

    pub fn mapping_report(&self) -> Option<&Report> {
        self.mapping_report.as_ref()
    }

    pub fn source(&self) -> &str {
        self.source
    }

    pub fn node_count(&self) -> usize {
        self.node_count
    }

    pub fn candidates(&self) -> &[NodeCandidate] {
        &self.candidates
    }
}

#[derive(Clone, Copy)]
pub struct AttachedTypeFacts<'a> {
    pub fact_index: usize,
    pub facts: &'a OccurrenceTypeFacts,
    graph: &'a Arc<TypeGraph>,
}

impl<'a> AttachedTypeFacts<'a> {
    pub fn graph(&self) -> &'a TypeGraph {
        self.graph.as_ref()
    }

    pub fn inspect(&self, limits: InspectorLimits) -> InspectionReport {
        GraphInspector::new(self.graph(), limits).inspect(self.facts)
    }
}

fn project_nodes(
    file: &str,
    source: &str,
    semantic: &Semantic<'_>,
) -> (Vec<NodeCandidate>, HashMap<u32, NodeId>) {
    let mut node_ids = HashMap::new();
    let candidates = semantic
        .nodes()
        .iter_enumerated()
        .filter_map(|(node_id, node)| {
            let kind = node.kind();
            let span = span(kind.span());
            let (syntax_kind, normalizations) = project_kind(kind, span, source)?;
            let portable_id = u32::try_from(node_id.index()).expect("OXC NodeId exceeds u32");
            node_ids.insert(portable_id, node_id);
            Some(NodeCandidate {
                node_id: portable_id,
                file: file.to_owned(),
                span,
                syntax_kind: syntax_kind.to_owned(),
                normalizations,
            })
        })
        .collect();
    (candidates, node_ids)
}

fn project_kind(
    kind: AstKind<'_>,
    canonical_span: Span,
    source: &str,
) -> Option<(&'static str, Vec<Normalization>)> {
    let exact = |kind| Some((kind, Vec::new()));
    match kind {
        AstKind::IdentifierReference(_) | AstKind::IdentifierName(_) => exact("KindIdentifier"),
        AstKind::BindingIdentifier(_) => Some((
            "KindBindingIdentifier",
            vec![Normalization {
                span: canonical_span,
                syntax_kind: "KindIdentifier".to_owned(),
                rule: NormalizationRule::KindAlias,
            }],
        )),
        AstKind::PrivateIdentifier(_) => exact("KindPrivateIdentifier"),
        AstKind::NumericLiteral(_) => exact("KindNumericLiteral"),
        AstKind::BigIntLiteral(_) => exact("KindBigIntLiteral"),
        AstKind::StringLiteral(_) => exact("KindStringLiteral"),
        AstKind::BooleanLiteral(literal) => Some((
            "KindBooleanLiteral",
            vec![Normalization {
                span: canonical_span,
                syntax_kind: if literal.value {
                    "KindTrueKeyword"
                } else {
                    "KindFalseKeyword"
                }
                .to_owned(),
                rule: NormalizationRule::KindAlias,
            }],
        )),
        AstKind::NullLiteral(_) => exact("KindNullKeyword"),
        AstKind::ParenthesizedExpression(expression) => {
            let inner = span(expression.expression.span());
            let syntax_kind = source
                .get(inner.start as usize..inner.end as usize)
                .filter(|text| is_identifier(text))
                .map(|_| "KindIdentifier")?;
            Some((
                "KindParenthesizedExpression",
                vec![Normalization {
                    span: inner,
                    syntax_kind: syntax_kind.to_owned(),
                    rule: NormalizationRule::ProtocolInnerSpan,
                }],
            ))
        }
        AstKind::JSXIdentifier(_) => exact("KindIdentifier"),
        _ => None,
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|character| character == '_' || character == '$' || character.is_alphabetic())
        && chars
            .all(|character| character == '_' || character == '$' || character.is_alphanumeric())
}

fn span(span: oxc_span::Span) -> Span {
    Span {
        start: span.start,
        end: span.end,
    }
}

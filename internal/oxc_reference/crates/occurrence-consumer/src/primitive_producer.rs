use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use oxc_allocator::Allocator;
use oxc_ast::{
    AstKind,
    ast::{Expression, TSLiteral, TSType, TSTypeName},
};
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::{NodeId, Semantic, SemanticBuilder};
use oxc_span::{GetSpan, SourceType};
use serde::Serialize;

use crate::{
    candidate::{
        CandidateFactStatus, CandidateReason, CandidateRoot, CandidateSemantic, CandidateState,
        CandidateSummary, CandidateTypeRecord, LiteralKind, NullLikeKind,
        PRIMITIVE_LITERAL_CANDIDATE_VERSION, PrimitiveKind, PrimitiveLiteralCandidate,
    },
    contract::{Occurrence, Span},
    facts::{TypeId, TypeView, TypeViewState},
};

pub const INDEPENDENT_PRIMITIVE_LITERAL_PRODUCER_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimitiveLiteralSelection {
    pub file: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveProducerLimits {
    pub max_type_nodes: usize,
}

impl Default for PrimitiveProducerLimits {
    fn default() -> Self {
        Self {
            max_type_nodes: 4096,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndependentPrimitiveLiteralOutput {
    pub producer_version: u32,
    pub candidate_version: u32,
    pub limits: PrimitiveProducerLimits,
    pub type_nodes_used: usize,
    pub truncated: bool,
    pub diagnostics: Vec<PrimitiveProducerDiagnostic>,
    pub candidates: Vec<PrimitiveLiteralCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveProducerDiagnostic {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    pub code: String,
    pub message: String,
}

pub fn produce_primitive_literals(
    project_root: &Path,
    selections: &[PrimitiveLiteralSelection],
    limits: PrimitiveProducerLimits,
) -> Result<IndependentPrimitiveLiteralOutput, String> {
    let mut sources = BTreeMap::new();
    for selection in selections {
        if !sources.contains_key(&selection.file) {
            let path = project_root.join(&selection.file);
            let source = fs::read_to_string(&path)
                .map_err(|error| format!("read project source {}: {error}", path.display()))?;
            sources.insert(selection.file.clone(), source);
        }
    }
    Ok(produce_primitive_literals_from_sources(
        &sources, selections, limits,
    ))
}

pub fn produce_primitive_literals_from_sources(
    sources: &BTreeMap<String, String>,
    selections: &[PrimitiveLiteralSelection],
    limits: PrimitiveProducerLimits,
) -> IndependentPrimitiveLiteralOutput {
    let mut producer = PrimitiveProducer::new(limits);
    let mut candidates = vec![None; selections.len()];

    for (file, source) in sources {
        let selected = selections
            .iter()
            .enumerate()
            .filter(|(_, selection)| selection.file == *file)
            .collect::<Vec<_>>();
        if selected.is_empty() {
            continue;
        }
        producer.produce_file(file, source, &selected, &mut candidates);
    }

    for (index, selection) in selections.iter().enumerate() {
        if candidates[index].is_none() {
            let message = if sources.contains_key(&selection.file) {
                "selection was not produced"
            } else {
                "selection source was not supplied"
            };
            producer.diagnostics.push(PrimitiveProducerDiagnostic {
                file: selection.file.clone(),
                span: Some(selection.span),
                code: "source-unavailable".to_owned(),
                message: message.to_owned(),
            });
            candidates[index] = Some(producer.failure_candidate(
                selection,
                "KindUnknown",
                CandidateState::Error,
                CandidateReason::OxcParseOrSemanticError,
            ));
        }
    }

    IndependentPrimitiveLiteralOutput {
        producer_version: INDEPENDENT_PRIMITIVE_LITERAL_PRODUCER_VERSION,
        candidate_version: PRIMITIVE_LITERAL_CANDIDATE_VERSION,
        limits,
        type_nodes_used: producer.interner.records.len(),
        truncated: producer.interner.truncated,
        diagnostics: producer.diagnostics,
        candidates: candidates
            .into_iter()
            .map(|candidate| candidate.expect("every selection receives a candidate"))
            .collect(),
    }
}

struct PrimitiveProducer {
    interner: TypeInterner,
    diagnostics: Vec<PrimitiveProducerDiagnostic>,
}

impl PrimitiveProducer {
    fn new(limits: PrimitiveProducerLimits) -> Self {
        Self {
            interner: TypeInterner::new(limits.max_type_nodes),
            diagnostics: Vec::new(),
        }
    }

    fn produce_file(
        &mut self,
        file: &str,
        source: &str,
        selections: &[(usize, &PrimitiveLiteralSelection)],
        candidates: &mut [Option<PrimitiveLiteralCandidate>],
    ) {
        let allocator = Allocator::default();
        let source_type = match SourceType::from_path(Path::new(file)) {
            Ok(source_type) => source_type,
            Err(_) => {
                self.fail_file(
                    file,
                    selections,
                    candidates,
                    "unsupported-source-type",
                    format!("unsupported source type for {file:?}"),
                );
                return;
            }
        };
        let parsed = Parser::new(&allocator, source, source_type)
            .with_options(ParseOptions {
                preserve_parens: true,
                ..ParseOptions::default()
            })
            .parse();
        if parsed.panicked {
            self.fail_file(
                file,
                selections,
                candidates,
                "oxc-parse-error",
                "OXC parser panicked before it could produce a recoverable tree".to_owned(),
            );
            return;
        }
        let mut recovered = !parsed.diagnostics.is_empty();
        if recovered {
            self.diagnostics.push(PrimitiveProducerDiagnostic {
                file: file.to_owned(),
                span: None,
                code: "oxc-parse-recovery".to_owned(),
                message: format!(
                    "OXC recovered a syntax tree with {} parser diagnostic(s)",
                    parsed.diagnostics.len()
                ),
            });
        }
        let built = SemanticBuilder::new_compiler()
            .with_build_nodes(true)
            .build(&parsed.program);
        if !built.diagnostics.is_empty() {
            recovered = true;
            self.diagnostics.push(PrimitiveProducerDiagnostic {
                file: file.to_owned(),
                span: None,
                code: "oxc-semantic-recovery".to_owned(),
                message: format!(
                    "OXC built semantics with {} diagnostic(s)",
                    built.diagnostics.len()
                ),
            });
        }
        let semantic = built.semantic;

        for (index, selection) in selections {
            let matched = semantic
                .nodes()
                .iter_enumerated()
                .find_map(|(node_id, node)| {
                    let kind = node.kind();
                    let node_span = portable_span(kind.span());
                    (node_span == selection.span)
                        .then(|| syntax_kind(kind).map(|syntax_kind| (node_id, kind, syntax_kind)))
                        .flatten()
                });
            let Some((node_id, kind, syntax_kind)) = matched else {
                self.diagnostics.push(PrimitiveProducerDiagnostic {
                    file: file.to_owned(),
                    span: Some(selection.span),
                    code: "selection-unmapped".to_owned(),
                    message: "no exact OXC semantic node matched the requested UTF-8 span"
                        .to_owned(),
                });
                candidates[*index] = Some(self.failure_candidate(
                    selection,
                    "KindUnknown",
                    CandidateState::Unsupported,
                    CandidateReason::SelectionUnmapped,
                ));
                continue;
            };
            candidates[*index] = Some(self.build_candidate(
                selection,
                node_id,
                kind,
                syntax_kind,
                &semantic,
                recovered,
            ));
        }
    }

    fn fail_file(
        &mut self,
        file: &str,
        selections: &[(usize, &PrimitiveLiteralSelection)],
        candidates: &mut [Option<PrimitiveLiteralCandidate>],
        code: &str,
        message: String,
    ) {
        self.diagnostics.push(PrimitiveProducerDiagnostic {
            file: file.to_owned(),
            span: None,
            code: code.to_owned(),
            message,
        });
        for (index, selection) in selections {
            candidates[*index] = Some(self.failure_candidate(
                selection,
                "KindUnknown",
                CandidateState::Error,
                CandidateReason::OxcParseOrSemanticError,
            ));
        }
    }

    fn build_candidate(
        &mut self,
        selection: &PrimitiveLiteralSelection,
        node_id: NodeId,
        kind: AstKind<'_>,
        syntax_kind: String,
        semantic: &Semantic<'_>,
        recovered: bool,
    ) -> PrimitiveLiteralCandidate {
        let mut resolving = BTreeSet::new();
        let actual = infer_kind(kind, node_id, semantic, &mut resolving);
        let subject = subject_kind(kind);
        let contextual = if subject == SubjectKind::Literal {
            contextual_shape(node_id, semantic, &mut resolving)
        } else {
            None
        };
        let apparent = apparent_shape(&actual);
        let root_shapes = [
            (TypeView::Actual, TypeViewState::Available, Some(actual)),
            (
                TypeView::Contextual,
                if contextual.is_some() {
                    TypeViewState::Available
                } else {
                    TypeViewState::Unavailable
                },
                contextual,
            ),
            (TypeView::Widened, TypeViewState::SameAsActual, None),
            match apparent {
                Some(apparent) => (TypeView::Apparent, TypeViewState::Available, Some(apparent)),
                None => (TypeView::Apparent, TypeViewState::SameAsActual, None),
            },
            (
                TypeView::Declared,
                if subject == SubjectKind::Identifier {
                    TypeViewState::SameAsActual
                } else {
                    TypeViewState::Inapplicable
                },
                None,
            ),
        ];
        let actual_id = self.interner.intern(root_shapes[0].2.as_ref().unwrap());
        let mut roots = Vec::with_capacity(5);
        for (view, state, shape) in root_shapes {
            let type_id = match state {
                TypeViewState::Available => shape.as_ref().map(|shape| self.interner.intern(shape)),
                TypeViewState::SameAsActual => Some(actual_id.clone()),
                TypeViewState::Inapplicable | TypeViewState::Unavailable => None,
            };
            roots.push(CandidateRoot {
                view,
                state,
                type_id,
            });
        }
        self.finish_candidate(
            Occurrence {
                file: selection.file.clone(),
                span: selection.span,
                syntax_kind,
            },
            Some(u32::try_from(node_id.index()).expect("OXC NodeId exceeds u32")),
            roots,
            recovered,
        )
    }

    fn failure_candidate(
        &mut self,
        selection: &PrimitiveLiteralSelection,
        syntax_kind: &str,
        state: CandidateState,
        reason: CandidateReason,
    ) -> PrimitiveLiteralCandidate {
        let shape = SemanticShape::Unavailable {
            key: format!(
                "{}:{}:{}:{}",
                selection.file, selection.span.start, selection.span.end, syntax_kind
            ),
            state,
            reason,
        };
        let actual = self.interner.intern(&shape);
        let roots = [
            (
                TypeView::Actual,
                TypeViewState::Available,
                Some(actual.clone()),
            ),
            (TypeView::Contextual, TypeViewState::Unavailable, None),
            (
                TypeView::Widened,
                TypeViewState::SameAsActual,
                Some(actual.clone()),
            ),
            (
                TypeView::Apparent,
                TypeViewState::SameAsActual,
                Some(actual.clone()),
            ),
            (TypeView::Declared, TypeViewState::Unavailable, None),
        ]
        .into_iter()
        .map(|(view, state, type_id)| CandidateRoot {
            view,
            state,
            type_id,
        })
        .collect();
        self.finish_candidate(
            Occurrence {
                file: selection.file.clone(),
                span: selection.span,
                syntax_kind: syntax_kind.to_owned(),
            },
            None,
            roots,
            false,
        )
    }

    fn finish_candidate(
        &self,
        occurrence: Occurrence,
        oxc_node_id: Option<u32>,
        roots: Vec<CandidateRoot>,
        recovered: bool,
    ) -> PrimitiveLiteralCandidate {
        let types = self.interner.reachable_records(&roots);
        let mut summary = CandidateSummary::default();
        for record in &types {
            summary.add(record.candidate_state);
        }
        PrimitiveLiteralCandidate {
            candidate_version: PRIMITIVE_LITERAL_CANDIDATE_VERSION,
            occurrence,
            oxc_node_id,
            fact: CandidateFactStatus {
                complete: !recovered
                    && summary.truncated == 0
                    && summary.unsupported == 0
                    && summary.error == 0,
                recovered,
                truncated: summary.truncated > 0,
            },
            roots,
            types,
            summary,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SubjectKind {
    Identifier,
    Literal,
    Other,
}

fn subject_kind(kind: AstKind<'_>) -> SubjectKind {
    match kind {
        AstKind::IdentifierReference(_) | AstKind::BindingIdentifier(_) => SubjectKind::Identifier,
        AstKind::BooleanLiteral(_)
        | AstKind::NullLiteral(_)
        | AstKind::NumericLiteral(_)
        | AstKind::BigIntLiteral(_)
        | AstKind::StringLiteral(_) => SubjectKind::Literal,
        _ => SubjectKind::Other,
    }
}

fn infer_kind(
    kind: AstKind<'_>,
    node_id: NodeId,
    semantic: &Semantic<'_>,
    resolving: &mut BTreeSet<(u32, u32)>,
) -> SemanticShape {
    match kind {
        AstKind::BooleanLiteral(literal) => boolean_literal(literal.value),
        AstKind::NullLiteral(_) => SemanticShape::NullLike(NullLikeKind::Null),
        AstKind::NumericLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::Number, number_value(literal.value))
        }
        AstKind::BigIntLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::Bigint, literal.value.to_string())
        }
        AstKind::StringLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::String, literal.value.to_string())
        }
        AstKind::IdentifierReference(identifier) => identifier
            .reference_id
            .get()
            .and_then(|reference_id| semantic.scoping().get_reference(reference_id).symbol_id())
            .map(|symbol_id| {
                infer_declaration(
                    semantic.symbol_declaration(symbol_id).kind(),
                    semantic,
                    resolving,
                )
            })
            .unwrap_or_else(|| {
                if identifier.name == "undefined" {
                    SemanticShape::NullLike(NullLikeKind::Undefined)
                } else {
                    unsupported(node_id, CandidateReason::UnsupportedExpression)
                }
            }),
        AstKind::BindingIdentifier(identifier) => identifier
            .symbol_id
            .get()
            .map(|symbol_id| {
                infer_declaration(
                    semantic.symbol_declaration(symbol_id).kind(),
                    semantic,
                    resolving,
                )
            })
            .unwrap_or_else(|| unsupported(node_id, CandidateReason::UnsupportedExpression)),
        AstKind::TSBooleanKeyword(_) => SemanticShape::Primitive(PrimitiveKind::Boolean),
        AstKind::TSStringKeyword(_) => SemanticShape::Primitive(PrimitiveKind::String),
        AstKind::TSNumberKeyword(_) => SemanticShape::Primitive(PrimitiveKind::Number),
        AstKind::TSBigIntKeyword(_) => SemanticShape::Primitive(PrimitiveKind::Bigint),
        AstKind::TSNullKeyword(_) => SemanticShape::NullLike(NullLikeKind::Null),
        AstKind::TSUndefinedKeyword(_) => SemanticShape::NullLike(NullLikeKind::Undefined),
        AstKind::TSVoidKeyword(_) => SemanticShape::NullLike(NullLikeKind::Void),
        AstKind::TSLiteralType(literal) => infer_ts_literal(&literal.literal),
        AstKind::TSUnionType(union) => canonical_union(
            union
                .types
                .iter()
                .map(|member| infer_ts_type(member, semantic, resolving))
                .collect(),
        ),
        _ => unsupported(node_id, CandidateReason::UnsupportedTypeForm),
    }
}

fn infer_declaration(
    kind: AstKind<'_>,
    semantic: &Semantic<'_>,
    resolving: &mut BTreeSet<(u32, u32)>,
) -> SemanticShape {
    match kind {
        AstKind::VariableDeclarator(declarator) => declarator
            .type_annotation
            .as_ref()
            .map(|annotation| infer_ts_type(&annotation.type_annotation, semantic, resolving))
            .or_else(|| {
                declarator
                    .init
                    .as_ref()
                    .map(|expression| infer_expression(expression, semantic, resolving))
            })
            .unwrap_or_else(|| {
                unavailable_span(declarator.span, CandidateReason::UnsupportedExpression)
            }),
        AstKind::TSTypeAliasDeclaration(alias) => {
            let key = (alias.span.start, alias.span.end);
            if !resolving.insert(key) {
                return unavailable_span(alias.span, CandidateReason::UnsupportedTypeForm);
            }
            let shape = infer_ts_type(&alias.type_annotation, semantic, resolving);
            resolving.remove(&key);
            shape
        }
        _ => unavailable_span(kind.span(), CandidateReason::UnsupportedTypeForm),
    }
}

fn infer_ts_type(
    r#type: &TSType<'_>,
    semantic: &Semantic<'_>,
    resolving: &mut BTreeSet<(u32, u32)>,
) -> SemanticShape {
    match r#type {
        TSType::TSBooleanKeyword(_) => SemanticShape::Primitive(PrimitiveKind::Boolean),
        TSType::TSStringKeyword(_) => SemanticShape::Primitive(PrimitiveKind::String),
        TSType::TSNumberKeyword(_) => SemanticShape::Primitive(PrimitiveKind::Number),
        TSType::TSBigIntKeyword(_) => SemanticShape::Primitive(PrimitiveKind::Bigint),
        TSType::TSNullKeyword(_) => SemanticShape::NullLike(NullLikeKind::Null),
        TSType::TSUndefinedKeyword(_) => SemanticShape::NullLike(NullLikeKind::Undefined),
        TSType::TSVoidKeyword(_) => SemanticShape::NullLike(NullLikeKind::Void),
        TSType::TSLiteralType(literal) => infer_ts_literal(&literal.literal),
        TSType::TSUnionType(union) => canonical_union(
            union
                .types
                .iter()
                .map(|member| infer_ts_type(member, semantic, resolving))
                .collect(),
        ),
        TSType::TSParenthesizedType(parenthesized) => {
            infer_ts_type(&parenthesized.type_annotation, semantic, resolving)
        }
        TSType::TSTypeReference(reference) => match &reference.type_name {
            TSTypeName::IdentifierReference(identifier) => identifier
                .reference_id
                .get()
                .and_then(|reference_id| semantic.scoping().get_reference(reference_id).symbol_id())
                .map(|symbol_id| {
                    infer_declaration(
                        semantic.symbol_declaration(symbol_id).kind(),
                        semantic,
                        resolving,
                    )
                })
                .unwrap_or_else(|| {
                    unavailable_span(reference.span, CandidateReason::UnsupportedTypeForm)
                }),
            TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => {
                unavailable_span(reference.span, CandidateReason::UnsupportedTypeForm)
            }
        },
        _ => unavailable_span(r#type.span(), CandidateReason::UnsupportedTypeForm),
    }
}

fn infer_ts_literal(literal: &TSLiteral<'_>) -> SemanticShape {
    match literal {
        TSLiteral::BooleanLiteral(literal) => boolean_literal(literal.value),
        TSLiteral::NumericLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::Number, number_value(literal.value))
        }
        TSLiteral::BigIntLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::Bigint, literal.value.to_string())
        }
        TSLiteral::StringLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::String, literal.value.to_string())
        }
        TSLiteral::TemplateLiteral(_) | TSLiteral::UnaryExpression(_) => {
            unavailable_span(literal.span(), CandidateReason::UnsupportedTypeForm)
        }
    }
}

fn infer_expression(
    expression: &Expression<'_>,
    semantic: &Semantic<'_>,
    resolving: &mut BTreeSet<(u32, u32)>,
) -> SemanticShape {
    match expression {
        Expression::BooleanLiteral(literal) => boolean_literal(literal.value),
        Expression::NullLiteral(_) => SemanticShape::NullLike(NullLikeKind::Null),
        Expression::NumericLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::Number, number_value(literal.value))
        }
        Expression::BigIntLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::Bigint, literal.value.to_string())
        }
        Expression::StringLiteral(literal) => {
            SemanticShape::Literal(LiteralKind::String, literal.value.to_string())
        }
        Expression::Identifier(identifier) => identifier
            .reference_id
            .get()
            .and_then(|reference_id| semantic.scoping().get_reference(reference_id).symbol_id())
            .map(|symbol_id| {
                infer_declaration(
                    semantic.symbol_declaration(symbol_id).kind(),
                    semantic,
                    resolving,
                )
            })
            .unwrap_or_else(|| {
                unavailable_span(identifier.span, CandidateReason::UnsupportedExpression)
            }),
        Expression::ParenthesizedExpression(parenthesized) => {
            infer_expression(&parenthesized.expression, semantic, resolving)
        }
        Expression::TSAsExpression(assertion) => {
            infer_ts_type(&assertion.type_annotation, semantic, resolving)
        }
        Expression::TSTypeAssertion(assertion) => {
            infer_ts_type(&assertion.type_annotation, semantic, resolving)
        }
        Expression::TSSatisfiesExpression(satisfies) => {
            infer_expression(&satisfies.expression, semantic, resolving)
        }
        _ => unavailable_span(expression.span(), CandidateReason::UnsupportedExpression),
    }
}

fn contextual_shape(
    node_id: NodeId,
    semantic: &Semantic<'_>,
    resolving: &mut BTreeSet<(u32, u32)>,
) -> Option<SemanticShape> {
    semantic.nodes().ancestor_kinds(node_id).find_map(|kind| {
        let AstKind::VariableDeclarator(declarator) = kind else {
            return None;
        };
        let annotation = declarator.type_annotation.as_ref()?;
        let shape = infer_ts_type(&annotation.type_annotation, semantic, resolving);
        (!matches!(shape, SemanticShape::NullLike(NullLikeKind::Null))).then_some(shape)
    })
}

fn apparent_shape(actual: &SemanticShape) -> Option<SemanticShape> {
    let primitive = match actual {
        SemanticShape::Primitive(primitive) => Some(*primitive),
        SemanticShape::Literal(literal, _) => Some(match literal {
            LiteralKind::Boolean => PrimitiveKind::Boolean,
            LiteralKind::String => PrimitiveKind::String,
            LiteralKind::Number => PrimitiveKind::Number,
            LiteralKind::Bigint => PrimitiveKind::Bigint,
        }),
        SemanticShape::NullLike(_)
        | SemanticShape::Union(_)
        | SemanticShape::Unavailable { .. } => None,
    }?;
    Some(SemanticShape::Unavailable {
        key: format!("apparent:{primitive:?}"),
        state: CandidateState::Truncated,
        reason: CandidateReason::ApparentTypeOutsideCategory,
    })
}

fn canonical_union(mut members: Vec<SemanticShape>) -> SemanticShape {
    members.sort_by_key(union_member_rank);
    members.dedup();
    SemanticShape::Union(members)
}

fn union_member_rank(member: &SemanticShape) -> u8 {
    match member {
        SemanticShape::Literal(LiteralKind::String, _) => 0,
        SemanticShape::Literal(LiteralKind::Number, _) => 1,
        SemanticShape::Literal(LiteralKind::Bigint, _) => 2,
        SemanticShape::Literal(LiteralKind::Boolean, _) => 3,
        SemanticShape::Primitive(PrimitiveKind::String) => 4,
        SemanticShape::Primitive(PrimitiveKind::Number) => 5,
        SemanticShape::Primitive(PrimitiveKind::Bigint) => 6,
        SemanticShape::Primitive(PrimitiveKind::Boolean) => 7,
        SemanticShape::NullLike(NullLikeKind::Null) => 8,
        SemanticShape::NullLike(NullLikeKind::Undefined) => 9,
        SemanticShape::NullLike(NullLikeKind::Void) => 10,
        SemanticShape::Union(_) => 11,
        SemanticShape::Unavailable { .. } => 12,
    }
}

fn boolean_literal(value: bool) -> SemanticShape {
    SemanticShape::Literal(LiteralKind::Boolean, value.to_string())
}

fn number_value(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn unsupported(node_id: NodeId, reason: CandidateReason) -> SemanticShape {
    SemanticShape::Unavailable {
        key: format!("node:{}", node_id.index()),
        state: CandidateState::Unsupported,
        reason,
    }
}

fn unavailable_span(span: oxc_span::Span, reason: CandidateReason) -> SemanticShape {
    SemanticShape::Unavailable {
        key: format!("span:{}:{}", span.start, span.end),
        state: CandidateState::Unsupported,
        reason,
    }
}

fn syntax_kind(kind: AstKind<'_>) -> Option<String> {
    Some(
        match kind {
            AstKind::IdentifierReference(_)
            | AstKind::IdentifierName(_)
            | AstKind::BindingIdentifier(_) => "KindIdentifier",
            AstKind::NumericLiteral(_) => "KindNumericLiteral",
            AstKind::BigIntLiteral(_) => "KindBigIntLiteral",
            AstKind::StringLiteral(_) => "KindStringLiteral",
            AstKind::BooleanLiteral(literal) if literal.value => "KindTrueKeyword",
            AstKind::BooleanLiteral(_) => "KindFalseKeyword",
            AstKind::NullLiteral(_) => "KindNullKeyword",
            AstKind::TSBooleanKeyword(_) => "KindBooleanKeyword",
            AstKind::TSStringKeyword(_) => "KindStringKeyword",
            AstKind::TSNumberKeyword(_) => "KindNumberKeyword",
            AstKind::TSBigIntKeyword(_) => "KindBigIntKeyword",
            AstKind::TSNullKeyword(_) => "KindNullKeyword",
            AstKind::TSUndefinedKeyword(_) => "KindUndefinedKeyword",
            AstKind::TSVoidKeyword(_) => "KindVoidKeyword",
            AstKind::TSLiteralType(_) => "KindLiteralType",
            AstKind::TSUnionType(_) => "KindUnionType",
            _ => return None,
        }
        .to_owned(),
    )
}

fn portable_span(span: oxc_span::Span) -> Span {
    Span {
        start: span.start,
        end: span.end,
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SemanticShape {
    Primitive(PrimitiveKind),
    Literal(LiteralKind, String),
    NullLike(NullLikeKind),
    Union(Vec<SemanticShape>),
    Unavailable {
        key: String,
        state: CandidateState,
        reason: CandidateReason,
    },
}

struct TypeInterner {
    max_type_nodes: usize,
    ids: BTreeMap<SemanticShape, TypeId>,
    records: BTreeMap<TypeId, CandidateTypeRecord>,
    truncated: bool,
    truncation_id: Option<TypeId>,
}

impl TypeInterner {
    fn new(max_type_nodes: usize) -> Self {
        Self {
            max_type_nodes,
            ids: BTreeMap::new(),
            records: BTreeMap::new(),
            truncated: false,
            truncation_id: None,
        }
    }

    fn intern(&mut self, shape: &SemanticShape) -> TypeId {
        if let Some(id) = self.ids.get(shape) {
            return id.clone();
        }
        if self.records.len() >= self.max_type_nodes {
            return self.truncation_id();
        }
        let semantic = match shape {
            SemanticShape::Primitive(primitive) => Some(CandidateSemantic::Primitive {
                primitive: *primitive,
            }),
            SemanticShape::Literal(literal, value) => Some(CandidateSemantic::Literal {
                literal: *literal,
                value: value.clone(),
            }),
            SemanticShape::NullLike(null_like) => Some(CandidateSemantic::NullLike {
                null_like: *null_like,
            }),
            SemanticShape::Union(members) => Some(CandidateSemantic::Union {
                members: members.iter().map(|member| self.intern(member)).collect(),
            }),
            SemanticShape::Unavailable { .. } => None,
        };
        if let Some(id) = self.ids.get(shape) {
            return id.clone();
        }
        if self.records.len() >= self.max_type_nodes {
            return self.truncation_id();
        }
        let id = TypeId(format!("type:{}", self.records.len() + 1));
        let (candidate_state, reasons) = match shape {
            SemanticShape::Unavailable { state, reason, .. } => (*state, vec![*reason]),
            _ => (CandidateState::Complete, Vec::new()),
        };
        self.ids.insert(shape.clone(), id.clone());
        self.records.insert(
            id.clone(),
            CandidateTypeRecord {
                id: id.clone(),
                candidate_state,
                semantic,
                reasons,
            },
        );
        id
    }

    fn truncation_id(&mut self) -> TypeId {
        self.truncated = true;
        if let Some(id) = &self.truncation_id {
            return id.clone();
        }
        let id = TypeId(format!("type:{}", self.records.len() + 1));
        self.records.insert(
            id.clone(),
            CandidateTypeRecord {
                id: id.clone(),
                candidate_state: CandidateState::Truncated,
                semantic: None,
                reasons: vec![CandidateReason::TypeBudgetExceeded],
            },
        );
        self.truncation_id = Some(id.clone());
        id
    }

    fn reachable_records(&self, roots: &[CandidateRoot]) -> Vec<CandidateTypeRecord> {
        let mut pending = roots
            .iter()
            .filter_map(|root| root.type_id.clone())
            .collect::<Vec<_>>();
        let mut reachable = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !reachable.insert(id.clone()) {
                continue;
            }
            if let Some(CandidateTypeRecord {
                semantic: Some(CandidateSemantic::Union { members }),
                ..
            }) = self.records.get(&id)
            {
                pending.extend(members.iter().cloned());
            }
        }
        reachable
            .into_iter()
            .filter_map(|id| self.records.get(&id).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
type LiteralUnion = "ready" | 42 | true | 42n | null | undefined;
type Unsupported = { value: string };
declare const union: LiteralUnion;
declare const booleanValue: boolean;
declare const stringValue: string;
declare const numberValue: number;
declare const bigintValue: bigint;
declare const unsupported: Unsupported;
const contextual: string = "context";
union;
booleanValue;
stringValue;
numberValue;
bigintValue;
unsupported;
"#;

    #[test]
    fn source_and_oxc_facts_produce_all_selected_semantics_deterministically() {
        let sources = BTreeMap::from([("src/primitives.ts".to_owned(), SOURCE.to_owned())]);
        let selections = [
            ("union", 1),
            ("booleanValue", 1),
            ("stringValue", 1),
            ("numberValue", 1),
            ("bigintValue", 1),
            ("\"context\"", 0),
            ("unsupported", 1),
        ]
        .into_iter()
        .map(|(text, occurrence)| selection(SOURCE, text, occurrence))
        .collect::<Vec<_>>();

        let first = produce_primitive_literals_from_sources(
            &sources,
            &selections,
            PrimitiveProducerLimits::default(),
        );
        let repeated = produce_primitive_literals_from_sources(
            &sources,
            &selections,
            PrimitiveProducerLimits::default(),
        );
        assert_eq!(
            serde_json::to_vec(&first).expect("serialize first output"),
            serde_json::to_vec(&repeated).expect("serialize repeated output")
        );
        assert!(first.diagnostics.is_empty());
        assert!(
            first
                .candidates
                .iter()
                .all(|candidate| { candidate.roots.len() == 5 && candidate.oxc_node_id.is_some() })
        );

        let semantics = first
            .candidates
            .iter()
            .flat_map(|candidate| &candidate.types)
            .filter_map(|record| record.semantic.as_ref())
            .collect::<Vec<_>>();
        for primitive in [
            PrimitiveKind::Boolean,
            PrimitiveKind::String,
            PrimitiveKind::Number,
            PrimitiveKind::Bigint,
        ] {
            assert!(semantics.iter().any(|semantic| {
                matches!(semantic, CandidateSemantic::Primitive { primitive: actual } if *actual == primitive)
            }));
        }
        for literal in [
            LiteralKind::Boolean,
            LiteralKind::String,
            LiteralKind::Number,
            LiteralKind::Bigint,
        ] {
            assert!(semantics.iter().any(|semantic| {
                matches!(semantic, CandidateSemantic::Literal { literal: actual, .. } if *actual == literal)
            }));
        }
        assert!(semantics.iter().any(|semantic| {
            matches!(
                semantic,
                CandidateSemantic::NullLike {
                    null_like: NullLikeKind::Null
                }
            )
        }));
        assert!(semantics.iter().any(|semantic| {
            matches!(
                semantic,
                CandidateSemantic::NullLike {
                    null_like: NullLikeKind::Undefined
                }
            )
        }));
        assert!(semantics.iter().any(|semantic| {
            matches!(semantic, CandidateSemantic::Union { members } if members.len() == 6)
        }));
        assert_eq!(first.candidates.last().unwrap().summary.unsupported, 1);
    }

    #[test]
    fn response_local_type_budget_emits_one_explicit_truncation_sentinel() {
        let sources = BTreeMap::from([("src/primitives.ts".to_owned(), SOURCE.to_owned())]);
        let selections = vec![selection(SOURCE, "union", 1)];
        let output = produce_primitive_literals_from_sources(
            &sources,
            &selections,
            PrimitiveProducerLimits { max_type_nodes: 2 },
        );
        assert!(output.truncated);
        assert_eq!(
            output
                .candidates
                .iter()
                .flat_map(|candidate| &candidate.types)
                .filter(|record| record
                    .reasons
                    .contains(&CandidateReason::TypeBudgetExceeded))
                .map(|record| record.id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            1
        );
    }

    #[test]
    fn recoverable_oxc_diagnostics_mark_independent_facts_recovered() {
        const RECOVERED_SOURCE: &str = "declare const value: null;\nvalue;\n\
             const duplicate = 1;\nconst duplicate = 2;\n";
        let sources =
            BTreeMap::from([("src/recovered.ts".to_owned(), RECOVERED_SOURCE.to_owned())]);
        let selections = vec![PrimitiveLiteralSelection {
            file: "src/recovered.ts".to_owned(),
            span: {
                let selection = selection(RECOVERED_SOURCE, "value", 1);
                selection.span
            },
        }];
        let output = produce_primitive_literals_from_sources(
            &sources,
            &selections,
            PrimitiveProducerLimits::default(),
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "oxc-semantic-recovery")
        );
        assert!(output.candidates[0].fact.recovered);
        assert!(!output.candidates[0].fact.complete);
        assert!(!output.candidates[0].fact.truncated);
    }

    fn selection(source: &str, text: &str, occurrence: usize) -> PrimitiveLiteralSelection {
        let start = source
            .match_indices(text)
            .nth(occurrence)
            .map(|(start, _)| start)
            .unwrap();
        PrimitiveLiteralSelection {
            file: "src/primitives.ts".to_owned(),
            span: Span {
                start: u32::try_from(start).unwrap(),
                end: u32::try_from(start + text.len()).unwrap(),
            },
        }
    }
}

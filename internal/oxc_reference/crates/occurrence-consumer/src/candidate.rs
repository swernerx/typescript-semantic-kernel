use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{
    contract::Occurrence,
    facts::{
        EntityState, GraphIssue, OccurrenceTypeFacts, TypeGraph, TypeId, TypeKind, TypeView,
        TypeViewState,
    },
};

pub const PRIMITIVE_LITERAL_CANDIDATE_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveLiteralCandidate {
    pub candidate_version: u32,
    pub occurrence: Occurrence,
    pub fact: CandidateFactStatus,
    pub roots: Vec<CandidateRoot>,
    pub types: Vec<CandidateTypeRecord>,
    pub summary: CandidateSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateFactStatus {
    pub complete: bool,
    pub recovered: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRoot {
    pub view: TypeView,
    pub state: TypeViewState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_id: Option<TypeId>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateState {
    Complete,
    Truncated,
    Unsupported,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateReason {
    EmptyUnion,
    MissingLiteralDetails,
    SourceError,
    SourceTruncated,
    SourceUnsupported,
    UnsupportedLiteralKind,
    UnsupportedTypeKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateTypeRecord {
    pub id: TypeId,
    pub source_kind: TypeKind,
    pub source_state: EntityState,
    pub candidate_state: CandidateState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<CandidateSemantic>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<CandidateReason>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<GraphIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CandidateSemantic {
    Primitive {
        primitive: PrimitiveKind,
    },
    Literal {
        literal: LiteralKind,
        value: String,
    },
    NullLike {
        #[serde(rename = "nullLike")]
        null_like: NullLikeKind,
    },
    Union {
        members: Vec<TypeId>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveKind {
    Boolean,
    String,
    Number,
    Bigint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LiteralKind {
    Boolean,
    String,
    Number,
    Bigint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NullLikeKind {
    Null,
    Undefined,
    Void,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSummary {
    pub complete: usize,
    pub truncated: usize,
    pub unsupported: usize,
    pub error: usize,
}

impl PrimitiveLiteralCandidate {
    pub fn build(graph: &TypeGraph, facts: &OccurrenceTypeFacts) -> Self {
        let roots = facts
            .roots()
            .into_iter()
            .map(|root| CandidateRoot {
                view: root.view,
                state: root.state,
                type_id: root.type_id.cloned(),
            })
            .collect::<Vec<_>>();
        let mut builder = CandidateBuilder::new(graph);
        for root in &roots {
            if let Some(type_id) = &root.type_id {
                builder.visit(type_id);
            }
        }
        let types = builder.records.into_values().collect::<Vec<_>>();
        let summary = types
            .iter()
            .fold(CandidateSummary::default(), |mut summary, record| {
                match record.candidate_state {
                    CandidateState::Complete => summary.complete += 1,
                    CandidateState::Truncated => summary.truncated += 1,
                    CandidateState::Unsupported => summary.unsupported += 1,
                    CandidateState::Error => summary.error += 1,
                }
                summary
            });
        Self {
            candidate_version: PRIMITIVE_LITERAL_CANDIDATE_VERSION,
            occurrence: facts.occurrence(),
            fact: CandidateFactStatus {
                complete: facts.complete,
                recovered: facts.recovered,
                truncated: facts.truncated,
            },
            roots,
            types,
            summary,
        }
    }
}

struct CandidateBuilder<'a> {
    graph: &'a TypeGraph,
    records: BTreeMap<TypeId, CandidateTypeRecord>,
    visiting: BTreeSet<TypeId>,
}

impl<'a> CandidateBuilder<'a> {
    fn new(graph: &'a TypeGraph) -> Self {
        Self {
            graph,
            records: BTreeMap::new(),
            visiting: BTreeSet::new(),
        }
    }

    fn visit(&mut self, id: &TypeId) -> CandidateState {
        if let Some(record) = self.records.get(id) {
            return record.candidate_state;
        }
        if !self.visiting.insert(id.clone()) {
            return CandidateState::Unsupported;
        }

        let source = self
            .graph
            .type_record(id)
            .expect("validated fact and type edges reference existing graph types");
        let mut reasons = BTreeSet::new();
        let mut child_state = CandidateState::Complete;
        let semantic = match source.type_kind {
            TypeKind::Boolean => Some(CandidateSemantic::Primitive {
                primitive: PrimitiveKind::Boolean,
            }),
            TypeKind::String => Some(CandidateSemantic::Primitive {
                primitive: PrimitiveKind::String,
            }),
            TypeKind::Number => Some(CandidateSemantic::Primitive {
                primitive: PrimitiveKind::Number,
            }),
            TypeKind::Bigint => Some(CandidateSemantic::Primitive {
                primitive: PrimitiveKind::Bigint,
            }),
            TypeKind::Null => Some(CandidateSemantic::NullLike {
                null_like: NullLikeKind::Null,
            }),
            TypeKind::Undefined => Some(CandidateSemantic::NullLike {
                null_like: NullLikeKind::Undefined,
            }),
            TypeKind::Void => Some(CandidateSemantic::NullLike {
                null_like: NullLikeKind::Void,
            }),
            TypeKind::Literal => match &source.literal {
                Some(literal) => match literal_kind(&literal.kind) {
                    Some(kind) => Some(CandidateSemantic::Literal {
                        literal: kind,
                        value: literal.value.clone(),
                    }),
                    None => {
                        reasons.insert(CandidateReason::UnsupportedLiteralKind);
                        None
                    }
                },
                None => {
                    reasons.insert(CandidateReason::MissingLiteralDetails);
                    None
                }
            },
            TypeKind::Union => {
                if source.members.is_empty() {
                    reasons.insert(CandidateReason::EmptyUnion);
                }
                for member in &source.members {
                    child_state = merge_state(child_state, self.visit(member));
                }
                Some(CandidateSemantic::Union {
                    members: source.members.clone(),
                })
            }
            _ => {
                reasons.insert(CandidateReason::UnsupportedTypeKind);
                None
            }
        };

        let source_state = match source.state {
            EntityState::Complete if source.truncated => {
                reasons.insert(CandidateReason::SourceTruncated);
                CandidateState::Truncated
            }
            EntityState::Complete => CandidateState::Complete,
            EntityState::Truncated => {
                reasons.insert(CandidateReason::SourceTruncated);
                CandidateState::Truncated
            }
            EntityState::Unsupported => {
                reasons.insert(CandidateReason::SourceUnsupported);
                CandidateState::Unsupported
            }
            EntityState::Error => {
                reasons.insert(CandidateReason::SourceError);
                CandidateState::Error
            }
        };
        let structural_state = if semantic.is_some()
            && !reasons.contains(&CandidateReason::EmptyUnion)
            && !reasons.contains(&CandidateReason::MissingLiteralDetails)
            && !reasons.contains(&CandidateReason::UnsupportedLiteralKind)
        {
            child_state
        } else {
            CandidateState::Unsupported
        };
        let candidate_state = merge_state(source_state, structural_state);
        let record = CandidateTypeRecord {
            id: id.clone(),
            source_kind: source.type_kind,
            source_state: source.state,
            candidate_state,
            semantic,
            reasons: reasons.into_iter().collect(),
            issues: source.issues.clone(),
        };
        self.visiting.remove(id);
        self.records.insert(id.clone(), record);
        candidate_state
    }
}

fn literal_kind(kind: &str) -> Option<LiteralKind> {
    match kind {
        "boolean" => Some(LiteralKind::Boolean),
        "string" => Some(LiteralKind::String),
        "number" => Some(LiteralKind::Number),
        "bigint" => Some(LiteralKind::Bigint),
        _ => None,
    }
}

fn merge_state(left: CandidateState, right: CandidateState) -> CandidateState {
    use CandidateState::{Complete, Error, Truncated, Unsupported};
    match (left, right) {
        (Error, _) | (_, Error) => Error,
        (Truncated, _) | (_, Truncated) => Truncated,
        (Unsupported, _) | (_, Unsupported) => Unsupported,
        (Complete, Complete) => Complete,
    }
}

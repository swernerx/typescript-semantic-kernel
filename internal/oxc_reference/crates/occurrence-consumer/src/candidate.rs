use serde::Serialize;

use crate::{
    contract::Occurrence,
    facts::{TypeId, TypeView, TypeViewState},
};

pub const PRIMITIVE_LITERAL_CANDIDATE_VERSION: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveLiteralCandidate {
    pub candidate_version: u32,
    pub occurrence: Occurrence,
    pub oxc_node_id: Option<u32>,
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
    ApparentTypeOutsideCategory,
    OxcParseOrSemanticError,
    SelectionUnmapped,
    TypeBudgetExceeded,
    UnsupportedExpression,
    UnsupportedTypeForm,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateTypeRecord {
    pub id: TypeId,
    pub candidate_state: CandidateState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<CandidateSemantic>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<CandidateReason>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveKind {
    Boolean,
    String,
    Number,
    Bigint,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LiteralKind {
    Boolean,
    String,
    Number,
    Bigint,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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

impl CandidateSummary {
    pub fn add(&mut self, state: CandidateState) {
        match state {
            CandidateState::Complete => self.complete += 1,
            CandidateState::Truncated => self.truncated += 1,
            CandidateState::Unsupported => self.unsupported += 1,
            CandidateState::Error => self.error += 1,
        }
    }
}

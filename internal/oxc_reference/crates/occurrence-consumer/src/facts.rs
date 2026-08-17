use std::{
    collections::{BTreeMap, BTreeSet},
    io::BufRead,
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::contract::{Occurrence, Span};

pub const SEMANTIC_FACTS_SCHEMA_VERSION: u32 = 1;
pub const UTF8_BYTE_OFFSETS: &str = "utf8-bytes";

macro_rules! graph_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

graph_id!(TypeId);
graph_id!(SymbolId);
graph_id!(SignatureId);
graph_id!(DeclarationId);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TypeViewState {
    Available,
    SameAsActual,
    Inapplicable,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TypeView {
    Actual,
    Contextual,
    Widened,
    Apparent,
    Declared,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeViewStates {
    pub actual: TypeViewState,
    pub contextual: TypeViewState,
    pub widened: TypeViewState,
    pub apparent: TypeViewState,
    pub declared: TypeViewState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OccurrenceTypeFacts {
    record: String,
    pub file: String,
    pub span: Span,
    pub syntax_kind: String,
    pub actual_type: TypeId,
    pub type_at_location: TypeId,
    #[serde(default)]
    pub annotation_type: Option<TypeId>,
    #[serde(default)]
    pub inferred_type: Option<TypeId>,
    #[serde(default)]
    pub contextual_type: Option<TypeId>,
    #[serde(default)]
    pub widened_type: Option<TypeId>,
    #[serde(default)]
    pub apparent_type: Option<TypeId>,
    #[serde(default)]
    pub declared_type: Option<TypeId>,
    #[serde(default)]
    pub narrowed_type: Option<TypeId>,
    #[serde(default)]
    pub constraint_type: Option<TypeId>,
    pub type_view_states: TypeViewStates,
    #[serde(default)]
    pub symbol: Option<SymbolId>,
    #[serde(default)]
    pub declarations: Vec<DeclarationId>,
    pub complete: bool,
    pub recovered: bool,
    pub truncated: bool,
}

impl OccurrenceTypeFacts {
    pub fn occurrence(&self) -> Occurrence {
        Occurrence {
            file: self.file.clone(),
            span: self.span,
            syntax_kind: self.syntax_kind.clone(),
        }
    }

    pub fn actual(&self) -> &TypeId {
        &self.actual_type
    }

    pub fn contextual(&self) -> Option<&TypeId> {
        self.effective_optional_root(
            self.contextual_type.as_ref(),
            self.type_view_states.contextual,
        )
    }

    pub fn widened(&self) -> Option<&TypeId> {
        self.effective_optional_root(self.widened_type.as_ref(), self.type_view_states.widened)
    }

    pub fn apparent(&self) -> Option<&TypeId> {
        self.effective_optional_root(self.apparent_type.as_ref(), self.type_view_states.apparent)
    }

    pub fn declared(&self) -> Option<&TypeId> {
        self.effective_optional_root(self.declared_type.as_ref(), self.type_view_states.declared)
    }

    pub fn roots(&self) -> [TypeRoot<'_>; 5] {
        [
            TypeRoot {
                view: TypeView::Actual,
                state: self.type_view_states.actual,
                type_id: Some(self.actual()),
            },
            TypeRoot {
                view: TypeView::Contextual,
                state: self.type_view_states.contextual,
                type_id: self.contextual(),
            },
            TypeRoot {
                view: TypeView::Widened,
                state: self.type_view_states.widened,
                type_id: self.widened(),
            },
            TypeRoot {
                view: TypeView::Apparent,
                state: self.type_view_states.apparent,
                type_id: self.apparent(),
            },
            TypeRoot {
                view: TypeView::Declared,
                state: self.type_view_states.declared,
                type_id: self.declared(),
            },
        ]
    }

    fn effective_optional_root<'a>(
        &'a self,
        explicit: Option<&'a TypeId>,
        state: TypeViewState,
    ) -> Option<&'a TypeId> {
        match state {
            TypeViewState::Available => explicit,
            TypeViewState::SameAsActual => Some(&self.actual_type),
            TypeViewState::Inapplicable | TypeViewState::Unavailable => None,
        }
    }

    fn validate(&self, index: usize) -> Result<(), String> {
        if self.record != "fact" {
            return Err(format!("facts[{index}] record must be \"fact\""));
        }
        if self.file.is_empty() || self.syntax_kind.is_empty() {
            return Err(format!("facts[{index}] requires file and syntaxKind"));
        }
        if self.span.end < self.span.start {
            return Err(format!("facts[{index}] has an invalid span"));
        }
        if self.actual_type != self.type_at_location {
            return Err(format!(
                "facts[{index}] actualType must equal typeAtLocation"
            ));
        }
        if self.type_view_states.actual != TypeViewState::Available {
            return Err(format!("facts[{index}] actual view must be available"));
        }
        for (name, id, state) in [
            (
                "contextual",
                self.contextual_type.as_ref(),
                self.type_view_states.contextual,
            ),
            (
                "widened",
                self.widened_type.as_ref(),
                self.type_view_states.widened,
            ),
            (
                "apparent",
                self.apparent_type.as_ref(),
                self.type_view_states.apparent,
            ),
            (
                "declared",
                self.declared_type.as_ref(),
                self.type_view_states.declared,
            ),
        ] {
            if (state == TypeViewState::Available) != id.is_some() {
                return Err(format!(
                    "facts[{index}] {name} root presence disagrees with its view state"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeRoot<'a> {
    pub view: TypeView,
    pub state: TypeViewState,
    pub type_id: Option<&'a TypeId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    Any,
    Array,
    Bigint,
    Boolean,
    Callable,
    Conditional,
    Error,
    Index,
    IndexedAccess,
    Intersection,
    Literal,
    Mapped,
    Never,
    NonPrimitive,
    Null,
    Number,
    Object,
    Opaque,
    Reference,
    String,
    StringMapping,
    Substitution,
    Symbol,
    TemplateLiteral,
    This,
    Truncated,
    Tuple,
    TypeParameter,
    Undefined,
    Union,
    UniqueSymbol,
    Unknown,
    Unsupported,
    Void,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityState {
    Complete,
    Truncated,
    Unsupported,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphIssue {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiteralValue {
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalTypeDetails {
    pub check_type: TypeId,
    pub extends_type: TypeId,
    pub true_type: TypeId,
    pub false_type: TypeId,
    #[serde(default)]
    pub infer_type_parameters: Vec<TypeId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MappedTypeDetails {
    pub type_parameter: TypeId,
    pub constraint_type: TypeId,
    #[serde(default)]
    pub name_type: Option<TypeId>,
    pub template_type: TypeId,
    #[serde(default)]
    pub modifiers_type: Option<TypeId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IndexedAccessTypeDetails {
    pub object_type: TypeId,
    pub index_type: TypeId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateLiteralTypeDetails {
    #[serde(default)]
    pub types: Vec<TypeId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubstitutionTypeDetails {
    pub base_type: TypeId,
    pub constraint: TypeId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TypeRecord {
    record: String,
    pub id: TypeId,
    pub type_kind: TypeKind,
    pub display: String,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub members: Vec<TypeId>,
    #[serde(default)]
    pub symbol: Option<SymbolId>,
    #[serde(default)]
    pub target: Option<TypeId>,
    #[serde(default)]
    pub type_arguments: Vec<TypeId>,
    #[serde(default)]
    pub constraint: Option<TypeId>,
    #[serde(default)]
    pub default: Option<TypeId>,
    #[serde(default)]
    pub properties: Vec<SymbolId>,
    #[serde(default)]
    pub call_signatures: Vec<SignatureId>,
    #[serde(default)]
    pub construct_signatures: Vec<SignatureId>,
    #[serde(default)]
    pub index_signatures: Vec<SignatureId>,
    #[serde(default)]
    pub literal: Option<LiteralValue>,
    #[serde(default)]
    pub conditional: Option<ConditionalTypeDetails>,
    #[serde(default)]
    pub mapped: Option<MappedTypeDetails>,
    #[serde(default)]
    pub indexed_access: Option<IndexedAccessTypeDetails>,
    #[serde(default)]
    pub template_literal: Option<TemplateLiteralTypeDetails>,
    #[serde(default)]
    pub substitution: Option<SubstitutionTypeDetails>,
    pub state: EntityState,
    #[serde(default)]
    pub issues: Vec<GraphIssue>,
    pub complete: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationRecord {
    record: String,
    pub id: DeclarationId,
    pub file: String,
    pub span: Span,
    pub syntax_kind: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SymbolRecord {
    record: String,
    pub id: SymbolId,
    pub name: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub declarations: Vec<DeclarationId>,
    #[serde(default)]
    pub aliased_symbol: Option<SymbolId>,
    #[serde(default)]
    pub r#type: Option<TypeId>,
    #[serde(default)]
    pub declared_type: Option<TypeId>,
    #[serde(default)]
    pub members: Vec<SymbolId>,
    pub state: EntityState,
    #[serde(default)]
    pub issues: Vec<GraphIssue>,
    pub complete: bool,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureKind {
    Call,
    Construct,
    Index,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignatureRecord {
    record: String,
    pub id: SignatureId,
    pub signature_kind: SignatureKind,
    #[serde(default)]
    pub declaration: Option<DeclarationId>,
    #[serde(default)]
    pub target: Option<SignatureId>,
    #[serde(default)]
    pub type_arguments: Vec<TypeId>,
    #[serde(default)]
    pub type_parameters: Vec<TypeId>,
    #[serde(default)]
    pub this_type: Option<TypeId>,
    #[serde(default)]
    pub parameters: Vec<SymbolId>,
    #[serde(default)]
    pub index_key_type: Option<TypeId>,
    pub return_type: TypeId,
    pub state: EntityState,
    #[serde(default)]
    pub issues: Vec<GraphIssue>,
    pub complete: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum GraphRef {
    Type(TypeId),
    Symbol(SymbolId),
    Signature(SignatureId),
    Declaration(DeclarationId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEdge {
    pub label: String,
    pub target: GraphRef,
}

#[derive(Debug)]
pub struct TypeGraph {
    types: BTreeMap<TypeId, TypeRecord>,
    declarations: BTreeMap<DeclarationId, DeclarationRecord>,
    symbols: BTreeMap<SymbolId, SymbolRecord>,
    signatures: BTreeMap<SignatureId, SignatureRecord>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRecordCounts {
    pub types: usize,
    pub declarations: usize,
    pub symbols: usize,
    pub signatures: usize,
    pub edges: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityStateCounts {
    pub complete: usize,
    pub truncated: usize,
    pub unsupported: usize,
    pub error: usize,
}

impl TypeGraph {
    pub fn type_record(&self, id: &TypeId) -> Option<&TypeRecord> {
        self.types.get(id)
    }

    pub fn declaration(&self, id: &DeclarationId) -> Option<&DeclarationRecord> {
        self.declarations.get(id)
    }

    pub fn symbol(&self, id: &SymbolId) -> Option<&SymbolRecord> {
        self.symbols.get(id)
    }

    pub fn signature(&self, id: &SignatureId) -> Option<&SignatureRecord> {
        self.signatures.get(id)
    }

    pub fn node_count(&self) -> usize {
        self.types.len() + self.declarations.len() + self.symbols.len() + self.signatures.len()
    }

    pub fn record_counts(&self) -> GraphRecordCounts {
        GraphRecordCounts {
            types: self.types.len(),
            declarations: self.declarations.len(),
            symbols: self.symbols.len(),
            signatures: self.signatures.len(),
            edges: self
                .references()
                .map(|reference| {
                    self.edges(&reference)
                        .expect("indexed graph node has an edge list")
                        .len()
                })
                .sum(),
        }
    }

    pub fn state_counts(&self) -> EntityStateCounts {
        self.types
            .values()
            .map(|record| record.state)
            .chain(self.symbols.values().map(|record| record.state))
            .chain(self.signatures.values().map(|record| record.state))
            .fold(EntityStateCounts::default(), |mut counts, state| {
                match state {
                    EntityState::Complete => counts.complete += 1,
                    EntityState::Truncated => counts.truncated += 1,
                    EntityState::Unsupported => counts.unsupported += 1,
                    EntityState::Error => counts.error += 1,
                }
                counts
            })
    }

    pub fn sharing_counts(&self) -> (usize, usize) {
        let targets = self
            .references()
            .flat_map(|reference| {
                self.edges(&reference)
                    .expect("indexed graph node has an edge list")
                    .into_iter()
                    .map(|edge| edge.target)
            })
            .collect::<Vec<_>>();
        let unique_targets = targets.iter().collect::<BTreeSet<_>>().len();
        (targets.len().saturating_sub(unique_targets), targets.len())
    }

    pub fn contains(&self, reference: &GraphRef) -> bool {
        match reference {
            GraphRef::Type(id) => self.types.contains_key(id),
            GraphRef::Symbol(id) => self.symbols.contains_key(id),
            GraphRef::Signature(id) => self.signatures.contains_key(id),
            GraphRef::Declaration(id) => self.declarations.contains_key(id),
        }
    }

    pub fn edges(&self, reference: &GraphRef) -> Option<Vec<GraphEdge>> {
        match reference {
            GraphRef::Type(id) => self.types.get(id).map(type_edges),
            GraphRef::Symbol(id) => self.symbols.get(id).map(symbol_edges),
            GraphRef::Signature(id) => self.signatures.get(id).map(signature_edges),
            GraphRef::Declaration(id) => self.declarations.get(id).map(|_| Vec::new()),
        }
    }

    fn validate(&self) -> Result<(), String> {
        for reference in self.references() {
            for edge in self.edges(&reference).expect("indexed graph node") {
                if !self.contains(&edge.target) {
                    return Err(format!(
                        "graph node {reference:?} edge {:?} references missing node {:?}",
                        edge.label, edge.target
                    ));
                }
            }
        }
        Ok(())
    }

    fn references(&self) -> impl Iterator<Item = GraphRef> + '_ {
        self.types
            .keys()
            .cloned()
            .map(GraphRef::Type)
            .chain(self.symbols.keys().cloned().map(GraphRef::Symbol))
            .chain(self.signatures.keys().cloned().map(GraphRef::Signature))
            .chain(self.declarations.keys().cloned().map(GraphRef::Declaration))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerBudgetLimits {
    pub max_type_nodes: u32,
    pub max_type_depth: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerBudgetReport {
    pub limits: ProducerBudgetLimits,
    pub type_nodes_used: u32,
    pub max_type_depth_observed: u32,
    pub truncated: bool,
}

#[derive(Debug)]
pub struct SemanticSnapshot {
    pub schema_version: u32,
    pub typescript_version: String,
    pub typescript_revision: String,
    pub offset_encoding: String,
    pub capabilities: Vec<String>,
    pub budgets: ProducerBudgetReport,
    pub diagnostic_count: u32,
    file_count: usize,
    graph: Arc<TypeGraph>,
    facts: Vec<OccurrenceTypeFacts>,
}

impl SemanticSnapshot {
    pub fn from_json_lines(reader: impl BufRead) -> Result<Self, String> {
        let mut header = None;
        let mut types = BTreeMap::new();
        let mut declarations = BTreeMap::new();
        let mut symbols = BTreeMap::new();
        let mut signatures = BTreeMap::new();
        let mut facts = Vec::new();
        let mut file_count = 0;

        for (line_index, line) in reader.lines().enumerate() {
            let line = line.map_err(|error| format!("read line {}: {error}", line_index + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(&line)
                .map_err(|error| format!("decode line {}: {error}", line_index + 1))?;
            let record = value
                .get("record")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("line {} requires record", line_index + 1))?;
            match record {
                "header" => {
                    if header.is_some() {
                        return Err("snapshot contains multiple header records".to_owned());
                    }
                    header = Some(
                        serde_json::from_value::<HeaderRecord>(value)
                            .map_err(|error| format!("decode header: {error}"))?,
                    );
                }
                "file" => file_count += 1,
                "type" => insert_record(
                    &mut types,
                    serde_json::from_value::<TypeRecord>(value)
                        .map_err(|error| format!("decode type: {error}"))?,
                    |record| &record.id,
                    "type",
                )?,
                "declaration" => insert_record(
                    &mut declarations,
                    serde_json::from_value::<DeclarationRecord>(value)
                        .map_err(|error| format!("decode declaration: {error}"))?,
                    |record| &record.id,
                    "declaration",
                )?,
                "symbol" => insert_record(
                    &mut symbols,
                    serde_json::from_value::<SymbolRecord>(value)
                        .map_err(|error| format!("decode symbol: {error}"))?,
                    |record| &record.id,
                    "symbol",
                )?,
                "signature" => insert_record(
                    &mut signatures,
                    serde_json::from_value::<SignatureRecord>(value)
                        .map_err(|error| format!("decode signature: {error}"))?,
                    |record| &record.id,
                    "signature",
                )?,
                "fact" => facts.push(
                    serde_json::from_value::<OccurrenceTypeFacts>(value)
                        .map_err(|error| format!("decode fact: {error}"))?,
                ),
                unknown => return Err(format!("unknown record kind {unknown:?}")),
            }
        }

        let header = header.ok_or_else(|| "snapshot requires a header record".to_owned())?;
        if header.schema_version != SEMANTIC_FACTS_SCHEMA_VERSION {
            return Err(format!(
                "unsupported schemaVersion {}; expected {}",
                header.schema_version, SEMANTIC_FACTS_SCHEMA_VERSION
            ));
        }
        if header.offset_encoding != UTF8_BYTE_OFFSETS {
            return Err(format!(
                "unsupported offsetEncoding {:?}; expected {:?}",
                header.offset_encoding, UTF8_BYTE_OFFSETS
            ));
        }

        let graph = Arc::new(TypeGraph {
            types,
            declarations,
            symbols,
            signatures,
        });
        graph.validate()?;
        for (index, fact) in facts.iter().enumerate() {
            fact.validate(index)?;
            for root in fact
                .roots()
                .into_iter()
                .filter_map(|root| root.type_id)
                .chain(fact.annotation_type.iter())
                .chain(fact.inferred_type.iter())
                .chain(fact.narrowed_type.iter())
                .chain(fact.constraint_type.iter())
            {
                if graph.type_record(root).is_none() {
                    return Err(format!(
                        "facts[{index}] references missing type {}",
                        root.as_str()
                    ));
                }
            }
            if let Some(symbol) = &fact.symbol
                && graph.symbol(symbol).is_none()
            {
                return Err(format!(
                    "facts[{index}] references missing symbol {}",
                    symbol.as_str()
                ));
            }
            let missing_declarations = fact
                .declarations
                .iter()
                .filter(|id| graph.declaration(id).is_none())
                .map(DeclarationId::as_str)
                .collect::<BTreeSet<_>>();
            if !missing_declarations.is_empty() {
                return Err(format!(
                    "facts[{index}] references missing declarations {missing_declarations:?}"
                ));
            }
        }

        Ok(Self {
            schema_version: header.schema_version,
            typescript_version: header.typescript_version,
            typescript_revision: header.typescript_revision,
            offset_encoding: header.offset_encoding,
            capabilities: header.capabilities,
            budgets: header.budgets,
            diagnostic_count: header.diagnostic_count,
            file_count,
            graph,
            facts,
        })
    }

    pub fn graph(&self) -> &Arc<TypeGraph> {
        &self.graph
    }

    pub fn facts(&self) -> &[OccurrenceTypeFacts] {
        &self.facts
    }

    pub fn file_count(&self) -> usize {
        self.file_count
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeaderRecord {
    schema_version: u32,
    #[serde(default)]
    typescript_version: String,
    #[serde(default)]
    typescript_revision: String,
    offset_encoding: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    budgets: ProducerBudgetReport,
    #[serde(default)]
    diagnostic_count: u32,
}

fn insert_record<K, V>(
    records: &mut BTreeMap<K, V>,
    record: V,
    id: impl FnOnce(&V) -> &K,
    kind: &str,
) -> Result<(), String>
where
    K: Clone + Ord + std::fmt::Debug,
{
    let id = id(&record).clone();
    if records.insert(id.clone(), record).is_some() {
        return Err(format!("duplicate {kind} id {id:?}"));
    }
    Ok(())
}

fn type_edges(record: &TypeRecord) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    push_many(&mut edges, "members", &record.members, GraphRef::Type);
    push_optional(&mut edges, "symbol", &record.symbol, GraphRef::Symbol);
    push_optional(&mut edges, "target", &record.target, GraphRef::Type);
    push_many(
        &mut edges,
        "typeArguments",
        &record.type_arguments,
        GraphRef::Type,
    );
    push_optional(&mut edges, "constraint", &record.constraint, GraphRef::Type);
    push_optional(&mut edges, "default", &record.default, GraphRef::Type);
    push_many(
        &mut edges,
        "properties",
        &record.properties,
        GraphRef::Symbol,
    );
    push_many(
        &mut edges,
        "callSignatures",
        &record.call_signatures,
        GraphRef::Signature,
    );
    push_many(
        &mut edges,
        "constructSignatures",
        &record.construct_signatures,
        GraphRef::Signature,
    );
    push_many(
        &mut edges,
        "indexSignatures",
        &record.index_signatures,
        GraphRef::Signature,
    );
    if let Some(details) = &record.conditional {
        push_required(
            &mut edges,
            "conditional.checkType",
            &details.check_type,
            GraphRef::Type,
        );
        push_required(
            &mut edges,
            "conditional.extendsType",
            &details.extends_type,
            GraphRef::Type,
        );
        push_required(
            &mut edges,
            "conditional.trueType",
            &details.true_type,
            GraphRef::Type,
        );
        push_required(
            &mut edges,
            "conditional.falseType",
            &details.false_type,
            GraphRef::Type,
        );
        push_many(
            &mut edges,
            "conditional.inferTypeParameters",
            &details.infer_type_parameters,
            GraphRef::Type,
        );
    }
    if let Some(details) = &record.mapped {
        push_required(
            &mut edges,
            "mapped.typeParameter",
            &details.type_parameter,
            GraphRef::Type,
        );
        push_required(
            &mut edges,
            "mapped.constraintType",
            &details.constraint_type,
            GraphRef::Type,
        );
        push_optional(
            &mut edges,
            "mapped.nameType",
            &details.name_type,
            GraphRef::Type,
        );
        push_required(
            &mut edges,
            "mapped.templateType",
            &details.template_type,
            GraphRef::Type,
        );
        push_optional(
            &mut edges,
            "mapped.modifiersType",
            &details.modifiers_type,
            GraphRef::Type,
        );
    }
    if let Some(details) = &record.indexed_access {
        push_required(
            &mut edges,
            "indexedAccess.objectType",
            &details.object_type,
            GraphRef::Type,
        );
        push_required(
            &mut edges,
            "indexedAccess.indexType",
            &details.index_type,
            GraphRef::Type,
        );
    }
    if let Some(details) = &record.template_literal {
        push_many(
            &mut edges,
            "templateLiteral.types",
            &details.types,
            GraphRef::Type,
        );
    }
    if let Some(details) = &record.substitution {
        push_required(
            &mut edges,
            "substitution.baseType",
            &details.base_type,
            GraphRef::Type,
        );
        push_required(
            &mut edges,
            "substitution.constraint",
            &details.constraint,
            GraphRef::Type,
        );
    }
    edges
}

fn symbol_edges(record: &SymbolRecord) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    push_many(
        &mut edges,
        "declarations",
        &record.declarations,
        GraphRef::Declaration,
    );
    push_optional(
        &mut edges,
        "aliasedSymbol",
        &record.aliased_symbol,
        GraphRef::Symbol,
    );
    push_optional(&mut edges, "type", &record.r#type, GraphRef::Type);
    push_optional(
        &mut edges,
        "declaredType",
        &record.declared_type,
        GraphRef::Type,
    );
    push_many(&mut edges, "members", &record.members, GraphRef::Symbol);
    edges
}

fn signature_edges(record: &SignatureRecord) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    push_optional(
        &mut edges,
        "declaration",
        &record.declaration,
        GraphRef::Declaration,
    );
    push_optional(&mut edges, "target", &record.target, GraphRef::Signature);
    push_many(
        &mut edges,
        "typeArguments",
        &record.type_arguments,
        GraphRef::Type,
    );
    push_many(
        &mut edges,
        "typeParameters",
        &record.type_parameters,
        GraphRef::Type,
    );
    push_optional(&mut edges, "thisType", &record.this_type, GraphRef::Type);
    push_many(
        &mut edges,
        "parameters",
        &record.parameters,
        GraphRef::Symbol,
    );
    push_optional(
        &mut edges,
        "indexKeyType",
        &record.index_key_type,
        GraphRef::Type,
    );
    push_required(
        &mut edges,
        "returnType",
        &record.return_type,
        GraphRef::Type,
    );
    edges
}

fn push_required<T: Clone>(
    edges: &mut Vec<GraphEdge>,
    label: &str,
    id: &T,
    reference: impl FnOnce(T) -> GraphRef,
) {
    edges.push(GraphEdge {
        label: label.to_owned(),
        target: reference(id.clone()),
    });
}

fn push_optional<T: Clone>(
    edges: &mut Vec<GraphEdge>,
    label: &str,
    id: &Option<T>,
    reference: impl FnOnce(T) -> GraphRef,
) {
    if let Some(id) = id {
        push_required(edges, label, id, reference);
    }
}

fn push_many<T: Clone>(
    edges: &mut Vec<GraphEdge>,
    label: &str,
    ids: &[T],
    reference: impl Fn(T) -> GraphRef,
) {
    for (index, id) in ids.iter().enumerate() {
        edges.push(GraphEdge {
            label: format!("{label}[{index}]"),
            target: reference(id.clone()),
        });
    }
}

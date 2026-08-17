use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::{
    candidate::{
        CandidateSemantic, CandidateState, CandidateSummary, CandidateTypeRecord, LiteralKind,
        NullLikeKind, PRIMITIVE_LITERAL_CANDIDATE_VERSION, PrimitiveKind,
        PrimitiveLiteralCandidate,
    },
    contract::{Occurrence, Span},
    facts::{
        EntityState, OccurrenceTypeFacts, ProducerBudgetReport, SemanticSnapshot, TypeGraph,
        TypeId, TypeKind, TypeViewState,
    },
    primitive_producer::{
        IndependentPrimitiveLiteralOutput, PrimitiveLiteralSelection, PrimitiveProducerLimits,
        produce_primitive_literals,
    },
};

pub const CONFORMANCE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConformanceReport {
    pub schema_version: u32,
    pub gate_kind: &'static str,
    pub candidate: &'static str,
    pub shadow_only: bool,
    pub threshold: CompatibilityThreshold,
    pub cases: Vec<ConformanceCase>,
    pub summary: ConformanceSummary,
    pub passes: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityThreshold {
    pub minimum_supported_records: usize,
    pub required_supported_compatibility_ppm: u64,
    pub max_unexplained_semantic_differences: usize,
    pub max_unexplained_transport_differences: usize,
    pub max_unexplained_mapping_differences: usize,
    pub unsupported_and_budget_differences_are_expected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConformanceCase {
    pub name: String,
    pub independent_scope: bool,
    pub facts: usize,
    pub repeated_rust_output_equal: bool,
    pub go_oracle: GoOracleEvidence,
    pub rust_producer: RustProducerEvidence,
    pub roots: RootCoverage,
    pub mapping: MappingCoverage,
    pub candidate_states: CandidateSummary,
    pub supported_records: usize,
    pub matched_supported_records: usize,
    pub supported_compatibility_ppm: u64,
    pub differences: Vec<ConformanceDifference>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoOracleEvidence {
    pub diagnostic_count: u32,
    pub budgets: ProducerBudgetReport,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RustProducerEvidence {
    pub diagnostic_count: usize,
    pub max_type_nodes: usize,
    pub type_nodes_used: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootCoverage {
    pub facts_with_all_five_views: usize,
    pub compared_roots: usize,
    pub identity_matches: usize,
    pub available: usize,
    pub same_as_actual: usize,
    pub inapplicable: usize,
    pub unavailable: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingCoverage {
    pub facts: usize,
    pub mapped: usize,
    pub unmapped: usize,
    pub failed_files: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConformanceSummary {
    pub cases: usize,
    pub facts: usize,
    pub candidate_records: usize,
    pub supported_records: usize,
    pub matched_supported_records: usize,
    pub supported_compatibility_ppm: u64,
    pub differences_by_category: BTreeMap<String, usize>,
    pub expected_differences: usize,
    pub blocking_differences: usize,
}

impl Default for ConformanceSummary {
    fn default() -> Self {
        Self {
            cases: 0,
            facts: 0,
            candidate_records: 0,
            supported_records: 0,
            matched_supported_records: 0,
            supported_compatibility_ppm: 0,
            differences_by_category: [
                ("semantic".to_owned(), 0),
                ("transport".to_owned(), 0),
                ("mapping".to_owned(), 0),
                ("unsupported".to_owned(), 0),
                ("budget".to_owned(), 0),
            ]
            .into_iter()
            .collect(),
            expected_differences: 0,
            blocking_differences: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DifferenceCategory {
    Semantic,
    Transport,
    Mapping,
    Unsupported,
    Budget,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConformanceDifference {
    pub category: DifferenceCategory,
    pub code: String,
    pub expected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<Occurrence>,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go_oracle: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_candidate: Option<serde_json::Value>,
    pub explanation: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorpusManifest {
    name: String,
    project: String,
    capabilities: Vec<String>,
    #[serde(default)]
    coverage: Vec<String>,
    #[serde(default)]
    budgets: ProducerBudgetRequest,
    selections: Vec<CorpusSelection>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProducerBudgetRequest {
    max_type_nodes: u32,
    max_type_depth: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct CorpusSelection {
    file: String,
    text: String,
    occurrence: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProducerRequest<'a> {
    schema_version: u32,
    project: &'a str,
    required_capabilities: &'a [String],
    budgets: ProducerBudgetRequest,
    selections: Vec<ProducerSelection>,
}

#[derive(Clone, Debug, Serialize)]
struct ProducerSelection {
    file: String,
    start: usize,
    end: usize,
}

pub fn run_conformance(
    tsfacts_binary: &Path,
    corpus_root: &Path,
) -> Result<ConformanceReport, String> {
    let tsfacts_binary = tsfacts_binary
        .canonicalize()
        .map_err(|error| format!("transport: resolve {}: {error}", tsfacts_binary.display()))?;
    let corpus_root = corpus_root
        .canonicalize()
        .map_err(|error| format!("transport: resolve {}: {error}", corpus_root.display()))?;
    let mut cases = Vec::new();
    for case_directory in sorted_case_directories(&corpus_root)? {
        let manifest = read_manifest(&case_directory)?;
        let request = build_request(&case_directory, &manifest)?;
        let snapshot = run_go_oracle(&tsfacts_binary, &case_directory, &request)?;
        let selections = request
            .selections
            .iter()
            .map(|selection| {
                Ok(PrimitiveLiteralSelection {
                    file: selection.file.clone(),
                    span: Span {
                        start: u32::try_from(selection.start)
                            .map_err(|_| "selection start exceeds u32")?,
                        end: u32::try_from(selection.end)
                            .map_err(|_| "selection end exceeds u32")?,
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let limits = PrimitiveProducerLimits {
            max_type_nodes: usize::try_from(manifest.budgets.max_type_nodes).unwrap_or(usize::MAX),
        };
        let first = produce_primitive_literals(&case_directory, &selections, limits)?;
        let repeated = produce_primitive_literals(&case_directory, &selections, limits)?;
        let repeated_equal = serde_json::to_vec(&first).ok() == serde_json::to_vec(&repeated).ok();
        let independent_scope = manifest
            .coverage
            .iter()
            .any(|item| item == "primitive-literals-independent");
        cases.push(compare_case(
            manifest.name,
            independent_scope,
            &snapshot,
            first,
            repeated_equal,
        ));
    }

    let summary = summarize(&cases);
    let threshold = compatibility_threshold();
    let passes = threshold_passes(&summary, threshold);
    Ok(ConformanceReport {
        schema_version: CONFORMANCE_SCHEMA_VERSION,
        gate_kind: "go-vs-independent-rust-semantic-conformance",
        candidate: "independent-primitive-literal-v2",
        shadow_only: true,
        threshold,
        cases,
        summary,
        passes,
    })
}

fn compatibility_threshold() -> CompatibilityThreshold {
    CompatibilityThreshold {
        minimum_supported_records: 15,
        required_supported_compatibility_ppm: 1_000_000,
        max_unexplained_semantic_differences: 0,
        max_unexplained_transport_differences: 0,
        max_unexplained_mapping_differences: 0,
        unsupported_and_budget_differences_are_expected: true,
    }
}

fn threshold_passes(summary: &ConformanceSummary, threshold: CompatibilityThreshold) -> bool {
    summary.supported_records >= threshold.minimum_supported_records
        && summary.supported_compatibility_ppm >= threshold.required_supported_compatibility_ppm
        && difference_count(summary, "semantic") <= threshold.max_unexplained_semantic_differences
        && difference_count(summary, "transport") <= threshold.max_unexplained_transport_differences
        && difference_count(summary, "mapping") <= threshold.max_unexplained_mapping_differences
}

fn difference_count(summary: &ConformanceSummary, category: &str) -> usize {
    summary
        .differences_by_category
        .get(category)
        .copied()
        .unwrap_or_default()
}

fn run_go_oracle(
    tsfacts_binary: &Path,
    case_directory: &Path,
    request: &ProducerRequest<'_>,
) -> Result<SemanticSnapshot, String> {
    let request_json = serde_json::to_vec(request)
        .map_err(|error| format!("transport: encode Go oracle request: {error}"))?;
    let mut child = Command::new(tsfacts_binary)
        .current_dir(case_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("transport: launch {}: {error}", tsfacts_binary.display()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "transport: Go oracle stdin is unavailable".to_owned())?
        .write_all(&request_json)
        .map_err(|error| format!("transport: write Go oracle request: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("transport: wait for Go oracle: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "transport: tsfacts exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    SemanticSnapshot::from_json_lines(BufReader::new(output.stdout.as_slice()))
        .map_err(|error| format!("transport: decode Go oracle output: {error}"))
}

fn compare_case(
    name: String,
    independent_scope: bool,
    snapshot: &SemanticSnapshot,
    output: IndependentPrimitiveLiteralOutput,
    repeated_equal: bool,
) -> ConformanceCase {
    let mut comparison = Comparison::new(snapshot, &output);
    if !repeated_equal {
        comparison.differences.push(mismatch(
            DifferenceCategory::Transport,
            "nondeterministic-rust-output",
            None,
            "rustProducer",
            json_value(&"first output"),
            json_value(&"different repeated output"),
            "the independent Rust producer must be byte-stable across repeated runs",
        ));
    }

    if independent_scope {
        comparison.compare_scoped_facts();
    } else {
        comparison.differences.push(expected_difference(
            DifferenceCategory::Unsupported,
            "case-outside-independent-slice",
            None,
            "case.coverage",
            None,
            None,
            "this pre-existing corpus case remains outside the selected primitive/literal slice",
        ));
    }
    if output.truncated {
        comparison.differences.push(expected_difference(
            DifferenceCategory::Budget,
            "rust-type-budget-truncated",
            None,
            "rustProducer.truncated",
            json_value(&snapshot.budgets.truncated),
            json_value(&true),
            "the independent Rust response-local type budget was exhausted",
        ));
    }
    comparison.finish(name, independent_scope, repeated_equal)
}

struct Comparison<'a> {
    snapshot: &'a SemanticSnapshot,
    output: &'a IndependentPrimitiveLiteralOutput,
    records: BTreeMap<TypeId, &'a CandidateTypeRecord>,
    roots: RootCoverage,
    mapping: MappingCoverage,
    candidate_states: CandidateSummary,
    bijection: TypeBijection,
    compared_pairs: BTreeSet<(TypeId, TypeId)>,
    supported_records: usize,
    matched_supported_records: usize,
    differences: Vec<ConformanceDifference>,
}

impl<'a> Comparison<'a> {
    fn new(snapshot: &'a SemanticSnapshot, output: &'a IndependentPrimitiveLiteralOutput) -> Self {
        let mut records = BTreeMap::new();
        let mut differences = Vec::new();
        let mut candidate_states = CandidateSummary::default();
        let mut seen_states = BTreeMap::new();
        for candidate in &output.candidates {
            for record in &candidate.types {
                if let Some(previous) = records.insert(record.id.clone(), record)
                    && previous != record
                {
                    differences.push(mismatch(
                        DifferenceCategory::Transport,
                        "inconsistent-response-local-record",
                        None,
                        &format!("types[{}]", record.id.as_str()),
                        json_value(previous),
                        json_value(record),
                        "one Rust response-local TypeID must identify one immutable record",
                    ));
                }
                seen_states
                    .entry(record.id.clone())
                    .or_insert(record.candidate_state);
            }
        }
        for state in seen_states.into_values() {
            candidate_states.add(state);
        }
        Self {
            snapshot,
            output,
            records,
            roots: RootCoverage::default(),
            mapping: MappingCoverage {
                facts: output.candidates.len(),
                ..MappingCoverage::default()
            },
            candidate_states,
            bijection: TypeBijection::default(),
            compared_pairs: BTreeSet::new(),
            supported_records: 0,
            matched_supported_records: 0,
            differences,
        }
    }

    fn compare_scoped_facts(&mut self) {
        let mut candidates = self
            .output
            .candidates
            .iter()
            .map(|candidate| (candidate.occurrence.clone(), candidate))
            .collect::<BTreeMap<_, _>>();
        for (fact_index, facts) in self.snapshot.facts().iter().enumerate() {
            let occurrence = facts.occurrence();
            let Some(candidate) = candidates.remove(&occurrence) else {
                self.mapping.unmapped += 1;
                self.differences.push(mismatch(
                    DifferenceCategory::Mapping,
                    "missing-rust-candidate",
                    Some((fact_index, occurrence)),
                    "mapping",
                    json_value(&"one candidate"),
                    json_value(&"none"),
                    "every selected Go fact must map to one independent OXC candidate",
                ));
                continue;
            };
            if candidate.oxc_node_id.is_some() {
                self.mapping.mapped += 1;
            } else {
                self.mapping.unmapped += 1;
                self.differences.push(mismatch(
                    DifferenceCategory::Mapping,
                    "missing-oxc-node",
                    Some((fact_index, occurrence.clone())),
                    "oxcNodeId",
                    json_value(&"typed OXC NodeId"),
                    None,
                    "independent facts must originate from an exact OXC semantic node",
                ));
            }
            self.compare_fact(fact_index, facts, candidate);
        }
        for (occurrence, _) in candidates {
            self.mapping.unmapped += 1;
            self.differences.push(mismatch(
                DifferenceCategory::Mapping,
                "extra-rust-candidate",
                None,
                "mapping",
                json_value(&"no extra candidate"),
                json_value(&occurrence),
                "the Rust producer must not invent facts outside the pinned request",
            ));
        }
    }

    fn compare_fact(
        &mut self,
        fact_index: usize,
        facts: &OccurrenceTypeFacts,
        candidate: &PrimitiveLiteralCandidate,
    ) {
        let context = Some((fact_index, facts.occurrence()));
        if candidate.candidate_version != PRIMITIVE_LITERAL_CANDIDATE_VERSION {
            self.differences.push(mismatch(
                DifferenceCategory::Transport,
                "candidate-version-mismatch",
                context.clone(),
                "candidateVersion",
                json_value(&PRIMITIVE_LITERAL_CANDIDATE_VERSION),
                json_value(&candidate.candidate_version),
                "the independent candidate version must be explicit and stable",
            ));
        }
        let actual_record = self.snapshot.graph().type_record(facts.actual());
        let actual_supported = actual_record.is_some_and(|record| {
            oracle_type_supported(self.snapshot.graph(), record, &mut BTreeSet::new())
        });
        if !actual_supported {
            self.differences.push(expected_difference(
                DifferenceCategory::Unsupported,
                "actual-type-outside-primitive-literal-slice",
                context,
                "roots[actual]",
                actual_record.map(oracle_record_value),
                candidate.roots.first().and_then(json_value),
                "the fixture intentionally retains an out-of-category selection",
            ));
            return;
        }

        let expected_fact = (facts.complete, facts.recovered, facts.truncated);
        let actual_fact = (
            candidate.fact.complete,
            candidate.fact.recovered,
            candidate.fact.truncated,
        );
        if expected_fact != actual_fact {
            self.differences.push(mismatch(
                DifferenceCategory::Transport,
                "fact-state-mismatch",
                context.clone(),
                "fact",
                json_value(&expected_fact),
                json_value(&actual_fact),
                "complete, recovered, and truncated state must be independently reproduced",
            ));
        }

        let expected_roots = facts.roots();
        self.roots.compared_roots += expected_roots.len();
        if candidate.roots.len() == 5 {
            self.roots.facts_with_all_five_views += 1;
        }
        if candidate.roots.len() != expected_roots.len() {
            self.differences.push(mismatch(
                DifferenceCategory::Transport,
                "root-count-mismatch",
                context.clone(),
                "roots",
                json_value(&expected_roots.len()),
                json_value(&candidate.roots.len()),
                "the independent candidate must expose exactly five ordered views",
            ));
        }
        for (index, expected) in expected_roots.iter().enumerate() {
            match expected.state {
                TypeViewState::Available => self.roots.available += 1,
                TypeViewState::SameAsActual => self.roots.same_as_actual += 1,
                TypeViewState::Inapplicable => self.roots.inapplicable += 1,
                TypeViewState::Unavailable => self.roots.unavailable += 1,
            }
            let Some(actual) = candidate.roots.get(index) else {
                continue;
            };
            if expected.view != actual.view || expected.state != actual.state {
                self.differences.push(mismatch(
                    DifferenceCategory::Transport,
                    "root-view-state-mismatch",
                    context.clone(),
                    &format!("roots[{index}]"),
                    json_value(&(expected.view, expected.state)),
                    json_value(&(actual.view, actual.state)),
                    "all five type-view states must be independently reproduced",
                ));
                continue;
            }
            match (expected.type_id, actual.type_id.as_ref()) {
                (None, None) => self.roots.identity_matches += 1,
                (Some(go_id), Some(rust_id)) => {
                    if self.compare_type(go_id, rust_id, fact_index, facts) {
                        self.roots.identity_matches += 1;
                    }
                }
                _ => self.differences.push(mismatch(
                    DifferenceCategory::Transport,
                    "root-identity-presence-mismatch",
                    context.clone(),
                    &format!("roots[{index}].typeId"),
                    expected.type_id.and_then(json_value),
                    actual.type_id.as_ref().and_then(json_value),
                    "available and same-as-actual roots must retain response-local identity",
                )),
            }
        }
    }

    fn compare_type(
        &mut self,
        go_id: &TypeId,
        rust_id: &TypeId,
        fact_index: usize,
        facts: &OccurrenceTypeFacts,
    ) -> bool {
        let context = Some((fact_index, facts.occurrence()));
        if !self.bijection.bind(go_id, rust_id) {
            self.differences.push(mismatch(
                DifferenceCategory::Transport,
                "response-local-identity-mismatch",
                context,
                &format!("types[{}]", go_id.as_str()),
                json_value(go_id),
                json_value(rust_id),
                "Go and Rust TypeIDs may differ in spelling but must form one response-wide bijection",
            ));
            return false;
        }
        let pair = (go_id.clone(), rust_id.clone());
        if !self.compared_pairs.insert(pair) {
            return true;
        }
        let Some(go_record) = self.snapshot.graph().type_record(go_id) else {
            return false;
        };
        if !oracle_type_supported(self.snapshot.graph(), go_record, &mut BTreeSet::new()) {
            return true;
        }
        self.supported_records += 1;
        let Some(rust_record) = self.records.get(rust_id).copied() else {
            self.differences.push(mismatch(
                DifferenceCategory::Transport,
                "missing-rust-type-record",
                context,
                &format!("types[{}]", rust_id.as_str()),
                Some(oracle_record_value(go_record)),
                None,
                "every Rust root and union member must resolve in the same response",
            ));
            return false;
        };
        if rust_record.candidate_state != CandidateState::Complete {
            self.differences.push(mismatch(
                DifferenceCategory::Semantic,
                "supported-type-downgraded",
                context,
                &format!("types[{}]", go_id.as_str()),
                Some(oracle_record_value(go_record)),
                json_value(rust_record),
                "a complete in-category Go type may not become unsupported or truncated in Rust",
            ));
            return false;
        }
        let matched = self.compare_semantic(go_record, rust_record, fact_index, facts);
        if matched {
            self.matched_supported_records += 1;
        }
        matched
    }

    fn compare_semantic(
        &mut self,
        go_record: &crate::facts::TypeRecord,
        rust_record: &CandidateTypeRecord,
        fact_index: usize,
        facts: &OccurrenceTypeFacts,
    ) -> bool {
        let context = Some((fact_index, facts.occurrence()));
        let matched = match (&go_record.type_kind, &rust_record.semantic) {
            (TypeKind::Boolean, Some(CandidateSemantic::Primitive { primitive })) => {
                *primitive == PrimitiveKind::Boolean
            }
            (TypeKind::String, Some(CandidateSemantic::Primitive { primitive })) => {
                *primitive == PrimitiveKind::String
            }
            (TypeKind::Number, Some(CandidateSemantic::Primitive { primitive })) => {
                *primitive == PrimitiveKind::Number
            }
            (TypeKind::Bigint, Some(CandidateSemantic::Primitive { primitive })) => {
                *primitive == PrimitiveKind::Bigint
            }
            (TypeKind::Null, Some(CandidateSemantic::NullLike { null_like })) => {
                *null_like == NullLikeKind::Null
            }
            (TypeKind::Undefined, Some(CandidateSemantic::NullLike { null_like })) => {
                *null_like == NullLikeKind::Undefined
            }
            (TypeKind::Void, Some(CandidateSemantic::NullLike { null_like })) => {
                *null_like == NullLikeKind::Void
            }
            (TypeKind::Literal, Some(CandidateSemantic::Literal { literal, value })) => {
                go_record.literal.as_ref().is_some_and(|go_literal| {
                    literal_wire_name(*literal) == go_literal.kind && *value == go_literal.value
                })
            }
            (TypeKind::Union, Some(CandidateSemantic::Union { members })) => {
                if go_record.members.len() != members.len() {
                    false
                } else {
                    go_record
                        .members
                        .iter()
                        .zip(members)
                        .all(|(go_member, rust_member)| {
                            self.compare_type(go_member, rust_member, fact_index, facts)
                        })
                }
            }
            _ => false,
        };
        if !matched {
            self.differences.push(mismatch(
                DifferenceCategory::Semantic,
                "primitive-literal-semantic-mismatch",
                context,
                &format!("types[{}].semantic", go_record.id.as_str()),
                Some(oracle_record_value(go_record)),
                json_value(rust_record),
                "structured primitive, literal, null-like, and union payloads must match",
            ));
        }
        matched
    }

    fn finish(
        mut self,
        name: String,
        independent_scope: bool,
        repeated_equal: bool,
    ) -> ConformanceCase {
        sort_differences(&mut self.differences);
        ConformanceCase {
            name,
            independent_scope,
            facts: self.snapshot.facts().len(),
            repeated_rust_output_equal: repeated_equal,
            go_oracle: GoOracleEvidence {
                diagnostic_count: self.snapshot.diagnostic_count,
                budgets: self.snapshot.budgets,
            },
            rust_producer: RustProducerEvidence {
                diagnostic_count: self.output.diagnostics.len(),
                max_type_nodes: self.output.limits.max_type_nodes,
                type_nodes_used: self.output.type_nodes_used,
                truncated: self.output.truncated,
            },
            roots: self.roots,
            mapping: self.mapping,
            candidate_states: self.candidate_states,
            supported_records: self.supported_records,
            matched_supported_records: self.matched_supported_records,
            supported_compatibility_ppm: ratio_ppm(
                self.matched_supported_records,
                self.supported_records,
            ),
            differences: self.differences,
        }
    }
}

#[derive(Default)]
struct TypeBijection {
    go_to_rust: BTreeMap<TypeId, TypeId>,
    rust_to_go: BTreeMap<TypeId, TypeId>,
}

impl TypeBijection {
    fn bind(&mut self, go: &TypeId, rust: &TypeId) -> bool {
        if self
            .go_to_rust
            .get(go)
            .is_some_and(|existing| existing != rust)
            || self
                .rust_to_go
                .get(rust)
                .is_some_and(|existing| existing != go)
        {
            return false;
        }
        self.go_to_rust.insert(go.clone(), rust.clone());
        self.rust_to_go.insert(rust.clone(), go.clone());
        true
    }
}

fn oracle_type_supported(
    graph: &TypeGraph,
    record: &crate::facts::TypeRecord,
    visiting: &mut BTreeSet<TypeId>,
) -> bool {
    if record.state != EntityState::Complete || !record.complete || record.truncated {
        return false;
    }
    match record.type_kind {
        TypeKind::Boolean
        | TypeKind::String
        | TypeKind::Number
        | TypeKind::Bigint
        | TypeKind::Null
        | TypeKind::Undefined
        | TypeKind::Void => true,
        TypeKind::Literal => record.literal.as_ref().is_some_and(|literal| {
            matches!(
                literal.kind.as_str(),
                "boolean" | "string" | "number" | "bigint"
            )
        }),
        TypeKind::Union => {
            if !visiting.insert(record.id.clone()) {
                return false;
            }
            let supported = !record.members.is_empty()
                && record.members.iter().all(|member| {
                    graph
                        .type_record(member)
                        .is_some_and(|record| oracle_type_supported(graph, record, visiting))
                });
            visiting.remove(&record.id);
            supported
        }
        _ => false,
    }
}

fn literal_wire_name(kind: LiteralKind) -> &'static str {
    match kind {
        LiteralKind::Boolean => "boolean",
        LiteralKind::String => "string",
        LiteralKind::Number => "number",
        LiteralKind::Bigint => "bigint",
    }
}

fn oracle_record_value(record: &crate::facts::TypeRecord) -> serde_json::Value {
    let literal = record.literal.as_ref().map(|literal| {
        serde_json::json!({
            "kind": literal.kind,
            "value": literal.value,
        })
    });
    serde_json::json!({
        "id": record.id.as_str(),
        "typeKind": record.type_kind,
        "members": record.members,
        "literal": literal,
        "state": record.state,
        "complete": record.complete,
        "truncated": record.truncated,
    })
}

fn summarize(cases: &[ConformanceCase]) -> ConformanceSummary {
    let mut summary = ConformanceSummary {
        cases: cases.len(),
        ..ConformanceSummary::default()
    };
    for case in cases {
        summary.facts += case.facts;
        summary.candidate_records += case.candidate_states.complete
            + case.candidate_states.truncated
            + case.candidate_states.unsupported
            + case.candidate_states.error;
        summary.supported_records += case.supported_records;
        summary.matched_supported_records += case.matched_supported_records;
        for difference in &case.differences {
            *summary
                .differences_by_category
                .entry(category_name(difference.category).to_owned())
                .or_default() += 1;
            if difference.expected {
                summary.expected_differences += 1;
            } else if matches!(
                difference.category,
                DifferenceCategory::Semantic
                    | DifferenceCategory::Transport
                    | DifferenceCategory::Mapping
            ) {
                summary.blocking_differences += 1;
            }
        }
    }
    summary.supported_compatibility_ppm =
        ratio_ppm(summary.matched_supported_records, summary.supported_records);
    summary
}

fn mismatch(
    category: DifferenceCategory,
    code: &str,
    context: Option<(usize, Occurrence)>,
    path: &str,
    go_oracle: Option<serde_json::Value>,
    rust_candidate: Option<serde_json::Value>,
    explanation: &str,
) -> ConformanceDifference {
    let (fact_index, occurrence) = context
        .map(|(index, occurrence)| (Some(index), Some(occurrence)))
        .unwrap_or((None, None));
    ConformanceDifference {
        category,
        code: code.to_owned(),
        expected: false,
        fact_index,
        occurrence,
        path: path.to_owned(),
        go_oracle,
        rust_candidate,
        explanation: explanation.to_owned(),
    }
}

fn expected_difference(
    category: DifferenceCategory,
    code: &str,
    context: Option<(usize, Occurrence)>,
    path: &str,
    go_oracle: Option<serde_json::Value>,
    rust_candidate: Option<serde_json::Value>,
    explanation: &str,
) -> ConformanceDifference {
    let mut difference = mismatch(
        category,
        code,
        context,
        path,
        go_oracle,
        rust_candidate,
        explanation,
    );
    difference.expected = true;
    difference
}

fn json_value(value: &impl Serialize) -> Option<serde_json::Value> {
    serde_json::to_value(value).ok()
}

fn sort_differences(differences: &mut [ConformanceDifference]) {
    differences.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then_with(|| left.fact_index.cmp(&right.fact_index))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| value_sort_key(&left.go_oracle).cmp(&value_sort_key(&right.go_oracle)))
            .then_with(|| {
                value_sort_key(&left.rust_candidate).cmp(&value_sort_key(&right.rust_candidate))
            })
    });
}

fn value_sort_key(value: &Option<serde_json::Value>) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn category_name(category: DifferenceCategory) -> &'static str {
    match category {
        DifferenceCategory::Semantic => "semantic",
        DifferenceCategory::Transport => "transport",
        DifferenceCategory::Mapping => "mapping",
        DifferenceCategory::Unsupported => "unsupported",
        DifferenceCategory::Budget => "budget",
    }
}

fn ratio_ppm(numerator: usize, denominator: usize) -> u64 {
    if denominator == 0 {
        0
    } else {
        (numerator as u64).saturating_mul(1_000_000) / denominator as u64
    }
}

fn sorted_case_directories(corpus_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut directories = fs::read_dir(corpus_root)
        .map_err(|error| format!("transport: read {}: {error}", corpus_root.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    directories.sort();
    Ok(directories)
}

fn read_manifest(case_directory: &Path) -> Result<CorpusManifest, String> {
    let path = case_directory.join("case.json");
    let source =
        fs::read(&path).map_err(|error| format!("transport: read {}: {error}", path.display()))?;
    serde_json::from_slice(&source)
        .map_err(|error| format!("transport: decode {}: {error}", path.display()))
}

fn build_request<'a>(
    case_directory: &Path,
    manifest: &'a CorpusManifest,
) -> Result<ProducerRequest<'a>, String> {
    let mut source_cache = BTreeMap::new();
    let mut selections = Vec::with_capacity(manifest.selections.len());
    for selection in &manifest.selections {
        let source = match source_cache.get(&selection.file) {
            Some(source) => source,
            None => {
                let path = case_directory.join(&selection.file);
                let source = fs::read_to_string(&path)
                    .map_err(|error| format!("transport: read {}: {error}", path.display()))?;
                source_cache.entry(selection.file.clone()).or_insert(source)
            }
        };
        let start = source
            .match_indices(&selection.text)
            .nth(selection.occurrence)
            .map(|(start, _)| start)
            .ok_or_else(|| {
                format!(
                    "transport: selection {:?} occurrence {} is absent from {}",
                    selection.text, selection.occurrence, selection.file
                )
            })?;
        selections.push(ProducerSelection {
            file: selection.file.clone(),
            start,
            end: start + selection.text.len(),
        });
    }
    Ok(ProducerRequest {
        schema_version: 1,
        project: &manifest.project,
        required_capabilities: &manifest.capabilities,
        budgets: manifest.budgets,
        selections,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_supported_denominator_never_passes() {
        let summary = ConformanceSummary {
            supported_compatibility_ppm: 1_000_000,
            ..ConformanceSummary::default()
        };
        assert!(!threshold_passes(&summary, compatibility_threshold()));
    }

    #[test]
    fn response_local_identity_comparison_is_bijective() {
        let mut bijection = TypeBijection::default();
        let go_one = TypeId("type:1".to_owned());
        let go_two = TypeId("type:2".to_owned());
        let rust_one = TypeId("type:8".to_owned());
        let rust_two = TypeId("type:9".to_owned());
        assert!(bijection.bind(&go_one, &rust_one));
        assert!(bijection.bind(&go_two, &rust_two));
        assert!(bijection.bind(&go_one, &rust_one));
        assert!(!bijection.bind(&go_one, &rust_two));
        assert!(!bijection.bind(&go_two, &rust_one));
    }
}

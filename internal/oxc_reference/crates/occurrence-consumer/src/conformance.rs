use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use oxc_allocator::Allocator;
use serde::{Deserialize, Serialize};

use crate::{
    candidate::{
        CandidateFactStatus, CandidateReason, CandidateRoot, CandidateSemantic, CandidateState,
        CandidateSummary, CandidateTypeRecord, LiteralKind, NullLikeKind,
        PRIMITIVE_LITERAL_CANDIDATE_VERSION, PrimitiveKind, PrimitiveLiteralCandidate,
    },
    contract::{DiagnosticCode, Occurrence},
    facts::{
        EntityState, OccurrenceTypeFacts, ProducerBudgetReport, SemanticSnapshot, TypeGraph,
        TypeId, TypeKind, TypeViewState,
    },
    oxc::OxcConsumer,
};

pub const CONFORMANCE_SCHEMA_VERSION: u32 = 1;

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
    pub mapping_differences_block_shadow_gate: bool,
    pub unsupported_and_budget_differences_are_expected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConformanceCase {
    pub name: String,
    pub facts: usize,
    pub diagnostics: DiagnosticComparison,
    pub budgets: BudgetComparison,
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
pub struct DiagnosticComparison {
    pub go_oracle_count: u32,
    pub rust_observed_count: u32,
    pub matched: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetComparison {
    pub go_oracle: ProducerBudgetReport,
    pub rust_observed: ProducerBudgetReport,
    pub matched: bool,
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
    pub multiply_mapped: usize,
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

struct RustCandidateRun {
    diagnostic_count: u32,
    budgets: ProducerBudgetReport,
    candidates: Vec<PrimitiveLiteralCandidate>,
}

impl RustCandidateRun {
    fn from_snapshot(snapshot: &SemanticSnapshot) -> Self {
        Self {
            diagnostic_count: snapshot.diagnostic_count,
            budgets: snapshot.budgets,
            candidates: snapshot
                .facts()
                .iter()
                .map(|facts| PrimitiveLiteralCandidate::build(snapshot.graph(), facts))
                .collect(),
        }
    }
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
        let source_files = request
            .selections
            .iter()
            .map(|selection| selection.file.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let snapshot = Arc::new(run_go_oracle(&tsfacts_binary, &case_directory, &request)?);
        let candidate_run = RustCandidateRun::from_snapshot(&snapshot);
        cases.push(compare_case(
            manifest.name,
            &case_directory,
            &source_files,
            &snapshot,
            candidate_run,
        )?);
    }

    let summary = summarize(&cases);
    let threshold = compatibility_threshold();
    let passes = threshold_passes(&summary, threshold);

    Ok(ConformanceReport {
        schema_version: CONFORMANCE_SCHEMA_VERSION,
        gate_kind: "go-vs-rust-semantic-conformance",
        candidate: "primitive-literal-v1",
        shadow_only: true,
        threshold,
        cases,
        summary,
        passes,
    })
}

fn compatibility_threshold() -> CompatibilityThreshold {
    CompatibilityThreshold {
        minimum_supported_records: 7,
        required_supported_compatibility_ppm: 1_000_000,
        max_unexplained_semantic_differences: 0,
        max_unexplained_transport_differences: 0,
        mapping_differences_block_shadow_gate: false,
        unsupported_and_budget_differences_are_expected: true,
    }
}

fn threshold_passes(summary: &ConformanceSummary, threshold: CompatibilityThreshold) -> bool {
    let semantic = summary
        .differences_by_category
        .get("semantic")
        .copied()
        .unwrap_or_default();
    let transport = summary
        .differences_by_category
        .get("transport")
        .copied()
        .unwrap_or_default();
    summary.supported_records >= threshold.minimum_supported_records
        && summary.supported_compatibility_ppm >= threshold.required_supported_compatibility_ppm
        && semantic <= threshold.max_unexplained_semantic_differences
        && transport <= threshold.max_unexplained_transport_differences
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
    case_directory: &Path,
    source_files: &[String],
    snapshot: &Arc<SemanticSnapshot>,
    candidate_run: RustCandidateRun,
) -> Result<ConformanceCase, String> {
    let mut differences = Vec::new();
    let mut roots = RootCoverage::default();
    let mut candidate_states = CandidateSummary::default();
    let mut supported_records = 0;
    let mut matched_supported_records = 0;

    compare_scalar(
        &mut differences,
        DifferenceCategory::Transport,
        "diagnostic-count-mismatch",
        None,
        "diagnostics.count",
        (snapshot.diagnostic_count, candidate_run.diagnostic_count),
        "Rust must preserve the Go oracle diagnostic count",
    );
    compare_scalar(
        &mut differences,
        DifferenceCategory::Transport,
        "budget-report-mismatch",
        None,
        "budgets",
        (snapshot.budgets, candidate_run.budgets),
        "Rust must preserve the Go oracle budget report",
    );
    if snapshot.budgets.truncated {
        differences.push(expected_difference(
            DifferenceCategory::Budget,
            "producer-budget-truncated",
            None,
            "budgets.truncated",
            json_value(&true),
            json_value(&true),
            "the corpus intentionally exercised the Go producer budget",
        ));
    }

    if snapshot.facts().len() != candidate_run.candidates.len() {
        differences.push(mismatch(
            DifferenceCategory::Transport,
            "fact-count-mismatch",
            None,
            "facts",
            json_value(&snapshot.facts().len()),
            json_value(&candidate_run.candidates.len()),
            "Rust must emit one candidate per Go fact",
        ));
    }
    for (fact_index, (facts, candidate)) in snapshot
        .facts()
        .iter()
        .zip(&candidate_run.candidates)
        .enumerate()
    {
        let compared = compare_fact(
            snapshot.graph(),
            fact_index,
            facts,
            candidate,
            &mut differences,
            &mut roots,
            &mut candidate_states,
        );
        supported_records += compared.0;
        matched_supported_records += compared.1;
    }

    let mapping = compare_mapping(case_directory, source_files, snapshot, &mut differences)?;
    sort_differences(&mut differences);
    let supported_compatibility_ppm = ratio_ppm(matched_supported_records, supported_records);
    Ok(ConformanceCase {
        name,
        facts: snapshot.facts().len(),
        diagnostics: DiagnosticComparison {
            go_oracle_count: snapshot.diagnostic_count,
            rust_observed_count: candidate_run.diagnostic_count,
            matched: snapshot.diagnostic_count == candidate_run.diagnostic_count,
        },
        budgets: BudgetComparison {
            go_oracle: snapshot.budgets,
            rust_observed: candidate_run.budgets,
            matched: snapshot.budgets == candidate_run.budgets,
        },
        roots,
        mapping,
        candidate_states,
        supported_records,
        matched_supported_records,
        supported_compatibility_ppm,
        differences,
    })
}

fn compare_mapping(
    case_directory: &Path,
    source_files: &[String],
    snapshot: &Arc<SemanticSnapshot>,
    differences: &mut Vec<ConformanceDifference>,
) -> Result<MappingCoverage, String> {
    let mut coverage = MappingCoverage {
        facts: snapshot.facts().len(),
        ..MappingCoverage::default()
    };
    for file in source_files {
        let path = case_directory.join(file);
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("mapping: read {}: {error}", path.display()))?;
        let allocator = Allocator::default();
        let mut consumer = match OxcConsumer::parse(&allocator, file, &source) {
            Ok(consumer) => consumer,
            Err(error) => {
                coverage.failed_files += 1;
                for (fact_index, facts) in snapshot
                    .facts()
                    .iter()
                    .enumerate()
                    .filter(|(_, facts)| facts.file == *file)
                {
                    differences.push(mismatch(
                        DifferenceCategory::Mapping,
                        "consumer-file-failed",
                        Some((fact_index, facts.occurrence())),
                        "mapping",
                        json_value(&"mapped Go fact"),
                        json_value(&"consumer file failed"),
                        &error,
                    ));
                }
                continue;
            }
        };
        let report = consumer
            .attach_file(Arc::clone(snapshot))
            .map_err(|error| format!("mapping: attach {file}: {error}"))?;
        coverage.mapped += report.summary.mapped as usize;
        coverage.unmapped += report.summary.unmapped as usize;
        coverage.multiply_mapped += report.summary.multiply_mapped as usize;
        for diagnostic in report.diagnostics {
            let facts = &snapshot.facts()[diagnostic.fact_index];
            let (code, explanation) = match diagnostic.code {
                DiagnosticCode::Unmapped => (
                    "unmapped-fact",
                    "the OXC syntax projection did not map this Go fact",
                ),
                DiagnosticCode::MultiplyMapped => (
                    "multiply-mapped-fact",
                    "the OXC syntax projection mapped this Go fact ambiguously",
                ),
            };
            differences.push(mismatch(
                DifferenceCategory::Mapping,
                code,
                Some((diagnostic.fact_index, facts.occurrence())),
                "mapping",
                json_value(&"one node"),
                json_value(&format!("{} nodes", diagnostic.candidates.len())),
                explanation,
            ));
        }
    }
    Ok(coverage)
}

fn compare_fact(
    graph: &TypeGraph,
    fact_index: usize,
    facts: &OccurrenceTypeFacts,
    candidate: &PrimitiveLiteralCandidate,
    differences: &mut Vec<ConformanceDifference>,
    root_coverage: &mut RootCoverage,
    state_summary: &mut CandidateSummary,
) -> (usize, usize) {
    let occurrence = facts.occurrence();
    let context = Some((fact_index, occurrence.clone()));
    compare_scalar(
        differences,
        DifferenceCategory::Transport,
        "candidate-version-mismatch",
        context.clone(),
        "candidateVersion",
        (
            PRIMITIVE_LITERAL_CANDIDATE_VERSION,
            candidate.candidate_version,
        ),
        "the shadow candidate version must be explicit and stable",
    );
    compare_scalar(
        differences,
        DifferenceCategory::Transport,
        "fact-identity-mismatch",
        context.clone(),
        "occurrence",
        (occurrence, candidate.occurrence.clone()),
        "file, UTF-8 span, and syntax kind identify the compared fact",
    );
    let expected_fact = CandidateFactStatus {
        complete: facts.complete,
        recovered: facts.recovered,
        truncated: facts.truncated,
    };
    compare_scalar(
        differences,
        DifferenceCategory::Transport,
        "fact-state-mismatch",
        context.clone(),
        "fact",
        (expected_fact, candidate.fact),
        "complete, recovered, and truncated fact states must survive transport",
    );
    if facts.truncated {
        differences.push(expected_difference(
            DifferenceCategory::Budget,
            "fact-truncated",
            context.clone(),
            "fact.truncated",
            json_value(&true),
            json_value(&candidate.fact.truncated),
            "the Go oracle marked this fact as truncated",
        ));
    }

    let expected_roots = facts
        .roots()
        .into_iter()
        .map(|root| CandidateRoot {
            view: root.view,
            state: root.state,
            type_id: root.type_id.cloned(),
        })
        .collect::<Vec<_>>();
    root_coverage.compared_roots += expected_roots.len();
    if expected_roots.len() == 5 {
        root_coverage.facts_with_all_five_views += 1;
    }
    for root in &expected_roots {
        match root.state {
            TypeViewState::Available => root_coverage.available += 1,
            TypeViewState::SameAsActual => root_coverage.same_as_actual += 1,
            TypeViewState::Inapplicable => root_coverage.inapplicable += 1,
            TypeViewState::Unavailable => root_coverage.unavailable += 1,
        }
    }
    for (index, expected) in expected_roots.iter().enumerate() {
        let actual = candidate.roots.get(index);
        if actual == Some(expected) {
            root_coverage.identity_matches += 1;
        } else {
            differences.push(mismatch(
                DifferenceCategory::Transport,
                "root-identity-mismatch",
                context.clone(),
                &format!("roots[{index}]"),
                json_value(expected),
                actual.and_then(json_value),
                "all actual, contextual, widened, apparent, and declared roots retain state and response-local TypeID",
            ));
        }
    }
    if candidate.roots.len() != expected_roots.len() {
        differences.push(mismatch(
            DifferenceCategory::Transport,
            "root-count-mismatch",
            context.clone(),
            "roots",
            json_value(&expected_roots.len()),
            json_value(&candidate.roots.len()),
            "the candidate must expose exactly five ordered type views",
        ));
    }

    let expected_records = OracleProjection::build(graph, facts);
    let actual_records = candidate
        .types
        .iter()
        .map(|record| (record.id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let actual_summary =
        summarize_states(candidate.types.iter().map(|record| record.candidate_state));
    state_summary.complete += actual_summary.complete;
    state_summary.truncated += actual_summary.truncated;
    state_summary.unsupported += actual_summary.unsupported;
    state_summary.error += actual_summary.error;
    let expected_summary = summarize_states(
        expected_records
            .values()
            .map(|record| record.candidate_state),
    );
    compare_scalar(
        differences,
        DifferenceCategory::Transport,
        "candidate-summary-mismatch",
        context.clone(),
        "summary",
        (expected_summary, candidate.summary),
        "candidate state totals must account for every projected TypeID",
    );

    let mut supported = 0;
    let mut matched_supported = 0;
    for (id, expected) in &expected_records {
        match expected.candidate_state {
            CandidateState::Complete => {
                supported += 1;
            }
            CandidateState::Truncated => {
                differences.push(expected_difference(
                    DifferenceCategory::Budget,
                    "candidate-type-truncated",
                    context.clone(),
                    &format!("types[{}]", id.as_str()),
                    json_value(expected),
                    actual_records
                        .get(id)
                        .and_then(|record| json_value(*record)),
                    "the source graph or a reachable union member exhausted its budget",
                ));
            }
            CandidateState::Unsupported => {
                differences.push(expected_difference(
                    DifferenceCategory::Unsupported,
                    "candidate-type-unsupported",
                    context.clone(),
                    &format!("types[{}]", id.as_str()),
                    json_value(expected),
                    actual_records
                        .get(id)
                        .and_then(|record| json_value(*record)),
                    "this graph form is outside primitive/literal-v1 or is explicitly unsupported",
                ));
            }
            CandidateState::Error => {
                differences.push(expected_difference(
                    DifferenceCategory::Unsupported,
                    "candidate-source-error",
                    context.clone(),
                    &format!("types[{}]", id.as_str()),
                    json_value(expected),
                    actual_records.get(id).and_then(|record| json_value(*record)),
                    "the Go oracle exposed an error-state type which remains ineligible for replacement",
                ));
            }
        }
        let Some(actual) = actual_records.get(id) else {
            differences.push(mismatch(
                DifferenceCategory::Transport,
                "missing-candidate-type",
                context.clone(),
                &format!("types[{}]", id.as_str()),
                json_value(expected),
                None,
                "the candidate omitted a response-local graph identity reachable through its supported projection",
            ));
            continue;
        };
        let exact = compare_type_record(differences, context.clone(), id, expected, actual);
        if expected.candidate_state == CandidateState::Complete && exact {
            matched_supported += 1;
        }
    }
    for (id, actual) in actual_records {
        if !expected_records.contains_key(&id) {
            differences.push(mismatch(
                DifferenceCategory::Transport,
                "unexpected-candidate-type",
                context.clone(),
                &format!("types[{}]", id.as_str()),
                None,
                json_value(actual),
                "the candidate fabricated a graph identity outside the Go-rooted projection",
            ));
        }
    }
    (supported, matched_supported)
}

fn compare_type_record(
    differences: &mut Vec<ConformanceDifference>,
    context: Option<(usize, Occurrence)>,
    id: &TypeId,
    expected: &CandidateTypeRecord,
    actual: &CandidateTypeRecord,
) -> bool {
    let before = differences.len();
    for (field, left, right) in [
        ("id", json_value(&expected.id), json_value(&actual.id)),
        (
            "sourceKind",
            json_value(&expected.source_kind),
            json_value(&actual.source_kind),
        ),
        (
            "sourceState",
            json_value(&expected.source_state),
            json_value(&actual.source_state),
        ),
        (
            "issues",
            json_value(&expected.issues),
            json_value(&actual.issues),
        ),
    ] {
        if left != right {
            differences.push(mismatch(
                DifferenceCategory::Transport,
                "type-source-mismatch",
                context.clone(),
                &format!("types[{}].{field}", id.as_str()),
                left,
                right,
                "source graph kind, state, issues, and identity must survive the Rust boundary",
            ));
        }
    }
    for (field, left, right) in [
        (
            "candidateState",
            json_value(&expected.candidate_state),
            json_value(&actual.candidate_state),
        ),
        (
            "semantic",
            json_value(&expected.semantic),
            json_value(&actual.semantic),
        ),
        (
            "reasons",
            json_value(&expected.reasons),
            json_value(&actual.reasons),
        ),
    ] {
        if left != right {
            differences.push(mismatch(
                DifferenceCategory::Semantic,
                "primitive-literal-semantic-mismatch",
                context.clone(),
                &format!("types[{}].{field}", id.as_str()),
                left,
                right,
                "structured primitive/literal payload, state, and reasons must equal the Go oracle projection",
            ));
        }
    }
    differences.len() == before
}

struct OracleProjection<'a> {
    graph: &'a TypeGraph,
    records: BTreeMap<TypeId, CandidateTypeRecord>,
    visiting: BTreeSet<TypeId>,
}

impl<'a> OracleProjection<'a> {
    fn build(
        graph: &'a TypeGraph,
        facts: &OccurrenceTypeFacts,
    ) -> BTreeMap<TypeId, CandidateTypeRecord> {
        let mut projection = Self {
            graph,
            records: BTreeMap::new(),
            visiting: BTreeSet::new(),
        };
        for root in facts.roots().into_iter().filter_map(|root| root.type_id) {
            projection.visit(root);
        }
        projection.records
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
            .expect("validated fact roots and union edges resolve");
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
                Some(literal) => match literal.kind.as_str() {
                    "boolean" => Some(CandidateSemantic::Literal {
                        literal: LiteralKind::Boolean,
                        value: literal.value.clone(),
                    }),
                    "string" => Some(CandidateSemantic::Literal {
                        literal: LiteralKind::String,
                        value: literal.value.clone(),
                    }),
                    "number" => Some(CandidateSemantic::Literal {
                        literal: LiteralKind::Number,
                        value: literal.value.clone(),
                    }),
                    "bigint" => Some(CandidateSemantic::Literal {
                        literal: LiteralKind::Bigint,
                        value: literal.value.clone(),
                    }),
                    _ => {
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

fn merge_state(left: CandidateState, right: CandidateState) -> CandidateState {
    use CandidateState::{Complete, Error, Truncated, Unsupported};
    match (left, right) {
        (Error, _) | (_, Error) => Error,
        (Truncated, _) | (_, Truncated) => Truncated,
        (Unsupported, _) | (_, Unsupported) => Unsupported,
        (Complete, Complete) => Complete,
    }
}

fn summarize_states(states: impl Iterator<Item = CandidateState>) -> CandidateSummary {
    states.fold(CandidateSummary::default(), |mut summary, state| {
        match state {
            CandidateState::Complete => summary.complete += 1,
            CandidateState::Truncated => summary.truncated += 1,
            CandidateState::Unsupported => summary.unsupported += 1,
            CandidateState::Error => summary.error += 1,
        }
        summary
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
            }
            if !difference.expected
                && matches!(
                    difference.category,
                    DifferenceCategory::Semantic | DifferenceCategory::Transport
                )
            {
                summary.blocking_differences += 1;
            }
        }
    }
    summary.supported_compatibility_ppm =
        ratio_ppm(summary.matched_supported_records, summary.supported_records);
    summary
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

fn compare_scalar<T: Serialize + PartialEq>(
    differences: &mut Vec<ConformanceDifference>,
    category: DifferenceCategory,
    code: &str,
    context: Option<(usize, Occurrence)>,
    path: &str,
    values: (T, T),
    explanation: &str,
) {
    let (expected, actual) = values;
    if expected != actual {
        differences.push(mismatch(
            category,
            code,
            context,
            path,
            json_value(&expected),
            json_value(&actual),
            explanation,
        ));
    }
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
    let (fact_index, occurrence) = match context {
        Some((index, occurrence)) => (Some(index), Some(occurrence)),
        None => (None, None),
    };
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
        1_000_000
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
    use std::{fs, io::BufReader, path::Path};

    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ClassificationFixture {
        description: String,
        cases: Vec<ClassificationCase>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ClassificationCase {
        mutation: String,
        expected_category: String,
        expected_code: String,
    }

    #[test]
    fn canonical_candidate_has_only_expected_unsupported_and_budget_differences() {
        let snapshot = canonical_snapshot();
        let run = RustCandidateRun::from_snapshot(&snapshot);
        let case = compare_without_mapping(&snapshot, run);

        assert_eq!(case.roots.facts_with_all_five_views, 4);
        assert_eq!(case.roots.compared_roots, 20);
        assert_eq!(case.roots.identity_matches, 20);
        assert!(case.supported_records > 0);
        assert_eq!(case.supported_records, case.matched_supported_records);
        assert_eq!(case.supported_compatibility_ppm, 1_000_000);
        assert!(case.differences.iter().any(|difference| {
            difference.category == DifferenceCategory::Unsupported && difference.expected
        }));
        assert!(case.differences.iter().any(|difference| {
            difference.category == DifferenceCategory::Budget && difference.expected
        }));
        assert!(case.differences.iter().all(|difference| {
            difference.expected
                || !matches!(
                    difference.category,
                    DifferenceCategory::Semantic | DifferenceCategory::Transport
                )
        }));
    }

    #[test]
    fn semantic_and_transport_mutations_are_blocking_and_deterministic() {
        let snapshot = canonical_snapshot();
        let fixture = classification_fixture();
        assert!(!fixture.description.is_empty());
        for fixture_case in fixture.cases {
            let mut first = RustCandidateRun::from_snapshot(&snapshot);
            match fixture_case.mutation.as_str() {
                "fact-identity" => {
                    first.candidates[0].occurrence.syntax_kind = "KindMutated".to_owned();
                }
                "root-identity" => {
                    first.candidates[0].roots[0].type_id = Some(TypeId("type:999".to_owned()));
                }
                "semantic-payload" => {
                    let record = first.candidates[0]
                        .types
                        .iter_mut()
                        .find(|record| record.candidate_state == CandidateState::Complete)
                        .expect("fixture has a supported record");
                    record.semantic = Some(CandidateSemantic::Primitive {
                        primitive: PrimitiveKind::String,
                    });
                }
                unknown => panic!("unknown classification mutation {unknown:?}"),
            }
            let second = first.clone_for_test();
            let first_case = compare_without_mapping(&snapshot, first);
            let second_case = compare_without_mapping(&snapshot, second);
            assert_eq!(
                serde_json::to_string(&first_case).expect("serialize first comparison"),
                serde_json::to_string(&second_case).expect("serialize repeated comparison"),
                "{}",
                fixture_case.mutation
            );
            assert!(first_case.differences.iter().any(|difference| {
                category_name(difference.category) == fixture_case.expected_category
                    && difference.code == fixture_case.expected_code
                    && !difference.expected
            }));
            let summary = summarize(&[first_case]);
            assert!(!threshold_passes(&summary, compatibility_threshold()));
        }
    }

    #[test]
    fn empty_supported_denominator_cannot_pass_vacuously() {
        let summary = ConformanceSummary {
            supported_compatibility_ppm: 1_000_000,
            ..ConformanceSummary::default()
        };
        assert!(!threshold_passes(&summary, compatibility_threshold()));
    }

    fn canonical_snapshot() -> SemanticSnapshot {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../../../internal/tsfacts/testdata/canonical/v0/primitive-literal-candidate.jsonl",
        );
        let file = fs::File::open(&path)
            .unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
        SemanticSnapshot::from_json_lines(BufReader::new(file)).expect("decode canonical fixture")
    }

    fn classification_fixture() -> ClassificationFixture {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/conformance/v1/classification.json");
        let source =
            fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        serde_json::from_slice(&source)
            .unwrap_or_else(|error| panic!("decode {}: {error}", path.display()))
    }

    fn compare_without_mapping(
        snapshot: &SemanticSnapshot,
        run: RustCandidateRun,
    ) -> ConformanceCase {
        let mut differences = Vec::new();
        let mut roots = RootCoverage::default();
        let mut candidate_states = CandidateSummary::default();
        let mut supported = 0;
        let mut matched = 0;
        for (index, (facts, candidate)) in snapshot.facts().iter().zip(&run.candidates).enumerate()
        {
            let compared = compare_fact(
                snapshot.graph(),
                index,
                facts,
                candidate,
                &mut differences,
                &mut roots,
                &mut candidate_states,
            );
            supported += compared.0;
            matched += compared.1;
        }
        sort_differences(&mut differences);
        ConformanceCase {
            name: "fixture".to_owned(),
            facts: snapshot.facts().len(),
            diagnostics: DiagnosticComparison {
                go_oracle_count: snapshot.diagnostic_count,
                rust_observed_count: run.diagnostic_count,
                matched: snapshot.diagnostic_count == run.diagnostic_count,
            },
            budgets: BudgetComparison {
                go_oracle: snapshot.budgets,
                rust_observed: run.budgets,
                matched: snapshot.budgets == run.budgets,
            },
            roots,
            mapping: MappingCoverage::default(),
            candidate_states,
            supported_records: supported,
            matched_supported_records: matched,
            supported_compatibility_ppm: ratio_ppm(matched, supported),
            differences,
        }
    }

    impl RustCandidateRun {
        fn clone_for_test(&self) -> Self {
            Self {
                diagnostic_count: self.diagnostic_count,
                budgets: self.budgets,
                candidates: self.candidates.clone(),
            }
        }
    }
}

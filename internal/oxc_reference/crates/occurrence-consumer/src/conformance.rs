use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use serde::{Deserialize, Serialize};

use crate::{
    candidate::{
        CandidateSemantic, CandidateState, CandidateSummary, CandidateTypeRecord, LiteralKind,
        NullLikeKind, PRIMITIVE_LITERAL_CANDIDATE_VERSION, PrimitiveKind,
        PrimitiveLiteralCandidate,
    },
    contract::{Occurrence, Span},
    evidence::resident_memory,
    facts::{
        EntityState, LiteralValue, OccurrenceTypeFacts, ProducerBudgetReport, SemanticSnapshot,
        TypeGraph, TypeId, TypeKind, TypeViewState, TypeViewStates,
    },
    primitive_producer::{
        IndependentPrimitiveLiteralOutput, PrimitiveLiteralSelection, PrimitiveProducerLimits,
        produce_primitive_literals,
    },
};

pub const CONFORMANCE_SCHEMA_VERSION: u32 = 5;
pub const ROLLOUT_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConformanceReport {
    pub schema_version: u32,
    pub gate_kind: &'static str,
    pub candidate: &'static str,
    pub shadow_only: bool,
    pub execution: ExecutionContract,
    pub threshold: CompatibilityThreshold,
    pub corpus: CorpusCoverage,
    pub cases: Vec<ConformanceCase>,
    pub summary: ConformanceSummary,
    pub passes: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionContract {
    pub repository_revision: String,
    pub typescript_version: String,
    pub typescript_revision: String,
    pub request_schema_version: u32,
    pub corpus_path: &'static str,
    pub go_semantic_authority: bool,
    pub rust_mode: &'static str,
    pub ts7_producer_protocol_changed: bool,
    pub external_consumer_behavior_changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RolloutReport {
    pub schema_version: u32,
    pub evidence_kind: &'static str,
    pub command: &'static str,
    pub environment: RolloutEnvironment,
    pub authority: AuthorityBoundary,
    pub determinism: DeterminismEvidence,
    pub conformance: ConformanceReport,
    pub measurements: RolloutMeasurements,
    pub readiness: AuthorityReadiness,
    pub passes: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RolloutEnvironment {
    pub operating_system: &'static str,
    pub architecture: &'static str,
    pub rustc: String,
    pub go: String,
    pub build_profile: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityBoundary {
    pub serving_authority: &'static str,
    pub production_fallback: &'static str,
    pub rust_mode: &'static str,
    pub authority_switch: bool,
    pub ts7_producer_protocol_changed: bool,
    pub external_consumer_behavior_changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeterminismEvidence {
    pub complete_runs: usize,
    pub conformance_reports_byte_equal: bool,
    pub compact_conformance_report_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RolloutMeasurements {
    pub scope: &'static str,
    pub samples: Vec<MeasurementSample>,
    pub artifacts: RolloutArtifacts,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasurementSample {
    pub ordinal: usize,
    pub cases: usize,
    pub go_oracle_nanoseconds: u64,
    pub rust_producer_nanoseconds: u64,
    pub rust_determinism_check_nanoseconds: u64,
    pub total_nanoseconds: u64,
    pub go_snapshot_bytes: usize,
    pub rust_candidate_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RolloutArtifacts {
    pub go_executable_bytes: u64,
    pub rust_executable_bytes: u64,
    pub peak_or_current_controller_resident_bytes: Option<u64>,
    pub resident_measurement: String,
    pub memory_scope: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityReadiness {
    pub ready_for_later_authority_decision: bool,
    pub status: &'static str,
    pub resolved_rollout_limitations: Vec<ResolvedRolloutLimitation>,
    pub blockers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRolloutLimitation {
    pub case: String,
    pub fact_index: usize,
    pub occurrence: Occurrence,
    pub classification: ExpectedClassification,
    pub code: String,
    pub stability: LimitationStability,
    pub owner: String,
    pub action: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusCoverage {
    pub discovered_cases: usize,
    pub selected_cases: usize,
    pub selected_facts: usize,
    pub excluded_cases: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityThreshold {
    pub minimum_supported_records: usize,
    pub required_supported_compatibility_ppm: u64,
    pub required_selection_accounting_ppm: u64,
    pub max_unexplained_semantic_differences: usize,
    pub max_unexplained_transport_differences: usize,
    pub max_unexplained_mapping_differences: usize,
    pub unsupported_and_budget_differences_are_expected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConformanceCase {
    pub name: String,
    pub facts: usize,
    pub request: PinnedRequestEvidence,
    pub repeated_rust_output_equal: bool,
    pub classifications: ClassificationCoverage,
    pub selections: Vec<SelectionEvidence>,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedRequestEvidence {
    pub schema_version: u32,
    pub project: String,
    pub required_capabilities: Vec<String>,
    pub budgets: ProducerBudgetRequest,
    pub selections: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationCoverage {
    pub supported: usize,
    pub unsupported: usize,
    pub budget: usize,
    pub mapping: usize,
}

impl ClassificationCoverage {
    fn add(&mut self, classification: ExpectedClassification) {
        match classification {
            ExpectedClassification::Supported => self.supported += 1,
            ExpectedClassification::Unsupported => self.unsupported += 1,
            ExpectedClassification::Budget => self.budget += 1,
            ExpectedClassification::Mapping => self.mapping += 1,
        }
    }

    fn merge(&mut self, other: Self) {
        self.supported += other.supported;
        self.unsupported += other.unsupported;
        self.budget += other.budget;
        self.mapping += other.mapping;
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionEvidence {
    pub fact_index: usize,
    pub proves: String,
    pub expected_classification: ExpectedClassification,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limitation_resolution: Option<LimitationResolution>,
    pub expectation_matched: bool,
    pub go_oracle: GoOracleFactObservation,
    pub rust_candidate: PrimitiveLiteralCandidate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoOracleFactObservation {
    pub occurrence: Occurrence,
    pub complete: bool,
    pub recovered: bool,
    pub truncated: bool,
    pub type_view_states: TypeViewStates,
    pub actual: serde_json::Value,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoOracleEvidence {
    pub typescript_version: String,
    pub typescript_revision: String,
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
    pub accounted_selections: usize,
    pub selection_accounting_ppm: u64,
    pub candidate_records: usize,
    pub supported_records: usize,
    pub matched_supported_records: usize,
    pub supported_compatibility_ppm: u64,
    pub classifications: ClassificationCoverage,
    pub differences_by_category: BTreeMap<String, usize>,
    pub unexplained_differences_by_category: BTreeMap<String, usize>,
    pub expected_differences: usize,
    pub blocking_differences: usize,
}

impl Default for ConformanceSummary {
    fn default() -> Self {
        Self {
            cases: 0,
            facts: 0,
            accounted_selections: 0,
            selection_accounting_ppm: 0,
            candidate_records: 0,
            supported_records: 0,
            matched_supported_records: 0,
            supported_compatibility_ppm: 0,
            classifications: ClassificationCoverage::default(),
            differences_by_category: [
                ("semantic".to_owned(), 0),
                ("transport".to_owned(), 0),
                ("mapping".to_owned(), 0),
                ("unsupported".to_owned(), 0),
                ("budget".to_owned(), 0),
            ]
            .into_iter()
            .collect(),
            unexplained_differences_by_category: [
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

struct NonSupportedObservation<'a> {
    category: DifferenceCategory,
    state_matches: bool,
    explanation: &'a str,
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerBudgetRequest {
    pub max_type_nodes: u32,
    pub max_type_depth: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct CorpusSelection {
    file: String,
    text: String,
    occurrence: usize,
    proves: String,
    #[serde(default)]
    conformance: Option<SelectionExpectation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectionExpectation {
    classification: ExpectedClassification,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    limitation_resolution: Option<LimitationResolution>,
    go_oracle: GoOracleExpectation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitationResolution {
    pub stability: LimitationStability,
    pub owner: String,
    pub action: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitationStability {
    Stable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpectedClassification {
    Supported,
    Unsupported,
    Budget,
    Mapping,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoOracleExpectation {
    complete: bool,
    recovered: bool,
    truncated: bool,
    type_view_states: TypeViewStates,
    actual: GoOracleTypeExpectation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoOracleTypeExpectation {
    type_kind: TypeKind,
    state: EntityState,
    complete: bool,
    truncated: bool,
    #[serde(default)]
    literal: Option<LiteralValue>,
    #[serde(default)]
    members: Option<Vec<GoOracleTypeExpectation>>,
    #[serde(default)]
    member_count: Option<usize>,
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

struct CompletedConformanceRun {
    report: ConformanceReport,
    measurement: MeasurementSample,
}

pub fn run_conformance(
    tsfacts_binary: &Path,
    corpus_root: &Path,
) -> Result<ConformanceReport, String> {
    run_conformance_at_revision(tsfacts_binary, corpus_root, "unrecorded")
}

pub fn run_conformance_at_revision(
    tsfacts_binary: &Path,
    corpus_root: &Path,
    repository_revision: &str,
) -> Result<ConformanceReport, String> {
    Ok(run_conformance_observed(tsfacts_binary, corpus_root, repository_revision)?.report)
}

pub fn run_rollout(
    tsfacts_binary: &Path,
    corpus_root: &Path,
    repository_revision: &str,
) -> Result<RolloutReport, String> {
    let mut first = run_conformance_observed(tsfacts_binary, corpus_root, repository_revision)?;
    let mut repeated = run_conformance_observed(tsfacts_binary, corpus_root, repository_revision)?;
    first.measurement.ordinal = 1;
    repeated.measurement.ordinal = 2;
    let first_bytes = serde_json::to_vec(&first.report)
        .map_err(|error| format!("transport: serialize first conformance report: {error}"))?;
    let repeated_bytes = serde_json::to_vec(&repeated.report)
        .map_err(|error| format!("transport: serialize repeated conformance report: {error}"))?;
    let reports_equal = first_bytes == repeated_bytes;
    let conformance_passes = first.report.passes && repeated.report.passes;
    let readiness = authority_readiness(&first.report.cases);
    let (resident_bytes, resident_measurement) = resident_memory();
    let rust_executable_bytes = std::env::current_exe()
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .map_or(0, |metadata| metadata.len());
    let go_executable_bytes = fs::metadata(tsfacts_binary).map_or(0, |metadata| metadata.len());

    Ok(RolloutReport {
        schema_version: ROLLOUT_SCHEMA_VERSION,
        evidence_kind: "primitive-literal-controlled-go-rust-dual-run",
        command: "./internal/oxc_reference/run-rollout.sh --output <path>",
        environment: RolloutEnvironment {
            operating_system: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            rustc: tool_version("rustc", &["--version"]),
            go: tool_version("go", &["version"]),
            build_profile: "release",
        },
        authority: AuthorityBoundary {
            serving_authority: "go",
            production_fallback: "go",
            rust_mode: "shadow-only",
            authority_switch: false,
            ts7_producer_protocol_changed: false,
            external_consumer_behavior_changed: false,
        },
        determinism: DeterminismEvidence {
            complete_runs: 2,
            conformance_reports_byte_equal: reports_equal,
            compact_conformance_report_bytes: first_bytes.len(),
        },
        conformance: first.report,
        measurements: RolloutMeasurements {
            scope: "one-shot Go process versus in-process Rust producer over identical ordered requests; characterization only",
            samples: vec![first.measurement, repeated.measurement],
            artifacts: RolloutArtifacts {
                go_executable_bytes,
                rust_executable_bytes,
                peak_or_current_controller_resident_bytes: resident_bytes,
                resident_measurement,
                memory_scope: "Rust rollout controller including decoded Go snapshots; excludes child Go process RSS",
            },
        },
        readiness,
        passes: conformance_passes && reports_equal,
    })
}

fn run_conformance_observed(
    tsfacts_binary: &Path,
    corpus_root: &Path,
    repository_revision: &str,
) -> Result<CompletedConformanceRun, String> {
    let tsfacts_binary = tsfacts_binary
        .canonicalize()
        .map_err(|error| format!("transport: resolve {}: {error}", tsfacts_binary.display()))?;
    let corpus_root = corpus_root
        .canonicalize()
        .map_err(|error| format!("transport: resolve {}: {error}", corpus_root.display()))?;
    let case_directories = sorted_case_directories(&corpus_root)?;
    let mut corpus = CorpusCoverage {
        discovered_cases: case_directories.len(),
        ..CorpusCoverage::default()
    };
    let run_started = Instant::now();
    let mut measurement = MeasurementSample::default();
    let mut cases = Vec::new();
    for case_directory in case_directories {
        let manifest = read_manifest(&case_directory)?;
        let expectation_count = manifest
            .selections
            .iter()
            .filter(|selection| selection.conformance.is_some())
            .count();
        if expectation_count == 0 {
            corpus.excluded_cases.push(manifest.name);
            continue;
        }
        if expectation_count != manifest.selections.len() {
            return Err(format!(
                "transport: case {:?} has {expectation_count}/{} conformance expectations; selected cases must classify every fixture",
                manifest.name,
                manifest.selections.len()
            ));
        }
        validate_expectations(&manifest)?;
        let request = build_request(&case_directory, &manifest)?;
        let request_evidence = PinnedRequestEvidence {
            schema_version: request.schema_version,
            project: request.project.to_owned(),
            required_capabilities: request.required_capabilities.to_vec(),
            budgets: request.budgets,
            selections: request.selections.len(),
        };
        let go_started = Instant::now();
        let (snapshot, snapshot_bytes) = run_go_oracle(&tsfacts_binary, &case_directory, &request)?;
        measurement.go_oracle_nanoseconds = measurement
            .go_oracle_nanoseconds
            .saturating_add(duration_nanoseconds(go_started.elapsed().as_nanos()));
        measurement.go_snapshot_bytes =
            measurement.go_snapshot_bytes.saturating_add(snapshot_bytes);
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
        let rust_started = Instant::now();
        let first = produce_primitive_literals(&case_directory, &selections, limits)?;
        measurement.rust_producer_nanoseconds = measurement
            .rust_producer_nanoseconds
            .saturating_add(duration_nanoseconds(rust_started.elapsed().as_nanos()));
        measurement.rust_candidate_bytes = measurement.rust_candidate_bytes.saturating_add(
            serde_json::to_vec(&first)
                .map_err(|error| format!("transport: serialize Rust candidate output: {error}"))?
                .len(),
        );
        let repeated_started = Instant::now();
        let repeated = produce_primitive_literals(&case_directory, &selections, limits)?;
        measurement.rust_determinism_check_nanoseconds = measurement
            .rust_determinism_check_nanoseconds
            .saturating_add(duration_nanoseconds(repeated_started.elapsed().as_nanos()));
        let repeated_equal = serde_json::to_vec(&first).ok() == serde_json::to_vec(&repeated).ok();
        let expectations = manifest
            .selections
            .iter()
            .map(|selection| {
                selection
                    .conformance
                    .clone()
                    .expect("selected cases require every expectation")
            })
            .collect::<Vec<_>>();
        let proves = manifest
            .selections
            .iter()
            .map(|selection| selection.proves.clone())
            .collect::<Vec<_>>();
        corpus.selected_cases += 1;
        corpus.selected_facts += expectations.len();
        cases.push(compare_case(
            manifest.name,
            &snapshot,
            first,
            repeated_equal,
            request_evidence,
            &expectations,
            &proves,
        ));
    }

    let summary = summarize(&cases);
    let threshold = compatibility_threshold();
    let passes = threshold_passes(&summary, threshold);
    let (typescript_version, typescript_revision) = compiler_identity(&cases)?;
    measurement.cases = cases.len();
    measurement.total_nanoseconds = duration_nanoseconds(run_started.elapsed().as_nanos());
    let report = ConformanceReport {
        schema_version: CONFORMANCE_SCHEMA_VERSION,
        gate_kind: "go-vs-independent-rust-semantic-conformance",
        candidate: "independent-primitive-literal-v2",
        shadow_only: true,
        execution: ExecutionContract {
            repository_revision: repository_revision.to_owned(),
            typescript_version,
            typescript_revision,
            request_schema_version: 1,
            corpus_path: "internal/semanticfacts/testdata/corpus/v0",
            go_semantic_authority: true,
            rust_mode: "shadow-only",
            ts7_producer_protocol_changed: false,
            external_consumer_behavior_changed: false,
        },
        threshold,
        corpus,
        cases,
        summary,
        passes,
    };
    Ok(CompletedConformanceRun {
        report,
        measurement,
    })
}

fn compatibility_threshold() -> CompatibilityThreshold {
    CompatibilityThreshold {
        minimum_supported_records: 15,
        required_supported_compatibility_ppm: 1_000_000,
        required_selection_accounting_ppm: 1_000_000,
        max_unexplained_semantic_differences: 0,
        max_unexplained_transport_differences: 0,
        max_unexplained_mapping_differences: 0,
        unsupported_and_budget_differences_are_expected: true,
    }
}

fn threshold_passes(summary: &ConformanceSummary, threshold: CompatibilityThreshold) -> bool {
    summary.supported_records >= threshold.minimum_supported_records
        && summary.supported_compatibility_ppm >= threshold.required_supported_compatibility_ppm
        && summary.selection_accounting_ppm == threshold.required_selection_accounting_ppm
        && difference_count(summary, "semantic") <= threshold.max_unexplained_semantic_differences
        && difference_count(summary, "transport") <= threshold.max_unexplained_transport_differences
        && difference_count(summary, "mapping") <= threshold.max_unexplained_mapping_differences
}

fn difference_count(summary: &ConformanceSummary, category: &str) -> usize {
    summary
        .unexplained_differences_by_category
        .get(category)
        .copied()
        .unwrap_or_default()
}

fn compiler_identity(cases: &[ConformanceCase]) -> Result<(String, String), String> {
    let Some(first) = cases.first() else {
        return Err("transport: conformance corpus selected no cases".to_owned());
    };
    let version = first.go_oracle.typescript_version.clone();
    let revision = first.go_oracle.typescript_revision.clone();
    if cases.iter().any(|case| {
        case.go_oracle.typescript_version != version
            || case.go_oracle.typescript_revision != revision
    }) {
        return Err(
            "transport: selected cases did not run against one pinned TypeScript compiler revision"
                .to_owned(),
        );
    }
    Ok((version, revision))
}

fn authority_readiness(cases: &[ConformanceCase]) -> AuthorityReadiness {
    let resolved_rollout_limitations = cases
        .iter()
        .flat_map(|case| {
            case.selections.iter().filter_map(|selection| {
                let resolution = selection.limitation_resolution.as_ref()?;
                Some(ResolvedRolloutLimitation {
                    case: case.name.clone(),
                    fact_index: selection.fact_index,
                    occurrence: selection.go_oracle.occurrence.clone(),
                    classification: selection.expected_classification,
                    code: selection.expected_code.clone().unwrap_or_default(),
                    stability: resolution.stability,
                    owner: resolution.owner.clone(),
                    action: resolution.action.clone(),
                })
            })
        })
        .collect();
    let blockers = vec![
        "the Rust producer is not integrated into the serving path, so production fallback and rollback have not been exercised"
            .to_owned(),
        "runtime and output measurements compare a one-shot Go process with an in-process Rust shadow path; a production-equivalent boundary is not selected"
            .to_owned(),
        "controller RSS excludes the child Go process, so per-producer peak-memory parity is not established"
            .to_owned(),
    ];
    AuthorityReadiness {
        ready_for_later_authority_decision: false,
        status: "not-ready",
        resolved_rollout_limitations,
        blockers,
    }
}

fn run_go_oracle(
    tsfacts_binary: &Path,
    case_directory: &Path,
    request: &ProducerRequest<'_>,
) -> Result<(SemanticSnapshot, usize), String> {
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
    let snapshot_bytes = output.stdout.len();
    let snapshot = SemanticSnapshot::from_json_lines(BufReader::new(output.stdout.as_slice()))
        .map_err(|error| format!("transport: decode Go oracle output: {error}"))?;
    Ok((snapshot, snapshot_bytes))
}

fn compare_case(
    name: String,
    snapshot: &SemanticSnapshot,
    output: IndependentPrimitiveLiteralOutput,
    repeated_equal: bool,
    request: PinnedRequestEvidence,
    expectations: &[SelectionExpectation],
    proves: &[String],
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

    comparison.compare_scoped_facts(expectations, proves);
    comparison.finish(name, repeated_equal, request)
}

struct Comparison<'a> {
    snapshot: &'a SemanticSnapshot,
    output: &'a IndependentPrimitiveLiteralOutput,
    records: BTreeMap<TypeId, &'a CandidateTypeRecord>,
    roots: RootCoverage,
    mapping: MappingCoverage,
    candidate_states: CandidateSummary,
    classifications: ClassificationCoverage,
    selections: Vec<SelectionEvidence>,
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
            classifications: ClassificationCoverage::default(),
            selections: Vec::new(),
            bijection: TypeBijection::default(),
            compared_pairs: BTreeSet::new(),
            supported_records: 0,
            matched_supported_records: 0,
            differences,
        }
    }

    fn compare_scoped_facts(&mut self, expectations: &[SelectionExpectation], proves: &[String]) {
        self.mapping.facts = expectations.len();
        if self.snapshot.facts().len() != expectations.len()
            || self.output.candidates.len() != expectations.len()
            || proves.len() != expectations.len()
        {
            self.differences.push(mismatch(
                DifferenceCategory::Transport,
                "selection-count-mismatch",
                None,
                "selections",
                json_value(&expectations.len()),
                json_value(&(
                    self.snapshot.facts().len(),
                    self.output.candidates.len(),
                    proves.len(),
                )),
                "the manifest, Go oracle, and Rust producer must retain one observation per selection",
            ));
        }

        let mut failed_files = BTreeSet::new();
        let count = expectations
            .len()
            .min(self.snapshot.facts().len())
            .min(self.output.candidates.len())
            .min(proves.len());
        for fact_index in 0..count {
            let facts = &self.snapshot.facts()[fact_index];
            let candidate = &self.output.candidates[fact_index];
            let expectation = &expectations[fact_index];
            let occurrence = facts.occurrence();
            let difference_start = self.differences.len();
            self.classifications.add(expectation.classification);
            self.validate_go_expectation(fact_index, facts, &expectation.go_oracle);

            if candidate.occurrence.file != occurrence.file
                || candidate.occurrence.span != occurrence.span
            {
                self.differences.push(mismatch(
                    DifferenceCategory::Transport,
                    "fact-identity-mismatch",
                    Some((fact_index, occurrence.clone())),
                    "occurrence",
                    json_value(&occurrence),
                    json_value(&candidate.occurrence),
                    "Go and Rust observations must retain the requested file and UTF-8 span",
                ));
            }

            match expectation.classification {
                ExpectedClassification::Supported => {
                    if candidate.occurrence.syntax_kind != occurrence.syntax_kind {
                        self.differences.push(mismatch(
                            DifferenceCategory::Mapping,
                            "syntax-kind-mismatch",
                            Some((fact_index, occurrence.clone())),
                            "occurrence.syntaxKind",
                            json_value(&occurrence.syntax_kind),
                            json_value(&candidate.occurrence.syntax_kind),
                            "supported observations require the exact portable syntax kind",
                        ));
                    }
                    if candidate.oxc_node_id.is_some() {
                        self.mapping.mapped += 1;
                        self.compare_fact(fact_index, facts, candidate);
                    } else {
                        self.mapping.unmapped += 1;
                        self.differences.push(mismatch(
                            DifferenceCategory::Mapping,
                            "missing-oxc-node",
                            Some((fact_index, occurrence.clone())),
                            "oxcNodeId",
                            json_value(&"typed OXC NodeId"),
                            None,
                            "supported facts must originate from an exact OXC semantic node",
                        ));
                    }
                }
                ExpectedClassification::Unsupported => {
                    self.observe_expected_non_supported(
                        fact_index,
                        facts,
                        candidate,
                        expectation,
                        NonSupportedObservation {
                            category: DifferenceCategory::Unsupported,
                            state_matches: candidate.summary.unsupported > 0,
                            explanation: "the fixture pins an out-of-category Rust observation",
                        },
                    );
                }
                ExpectedClassification::Budget => {
                    self.observe_expected_non_supported(
                        fact_index,
                        facts,
                        candidate,
                        expectation,
                        NonSupportedObservation {
                            category: DifferenceCategory::Budget,
                            state_matches: candidate.summary.truncated > 0 && self.output.truncated,
                            explanation: "the fixture pins explicit response-local budget truncation",
                        },
                    );
                }
                ExpectedClassification::Mapping => {
                    self.mapping.unmapped += 1;
                    failed_files.insert(facts.file.clone());
                    let code = expectation
                        .code
                        .as_deref()
                        .expect("mapping expectations require a code");
                    let diagnostic_matches =
                        self.output.diagnostics.iter().any(|diagnostic| {
                            diagnostic.file == facts.file && diagnostic.code == code
                        });
                    if candidate.oxc_node_id.is_none()
                        && candidate.summary.error > 0
                        && diagnostic_matches
                    {
                        self.differences.push(expected_difference(
                            DifferenceCategory::Mapping,
                            code,
                            Some((fact_index, occurrence.clone())),
                            "classification",
                            json_value(&expectation.classification),
                            json_value(candidate),
                            "the known recovery-file OXC parser gap is explicit and stable",
                        ));
                    } else {
                        self.differences.push(mismatch(
                            DifferenceCategory::Mapping,
                            "mapping-classification-mismatch",
                            Some((fact_index, occurrence.clone())),
                            "classification",
                            json_value(expectation),
                            json_value(candidate),
                            "the expected mapping gap changed and its fixture classification must be reviewed",
                        ));
                    }
                }
            }

            let expectation_matched = self.differences[difference_start..]
                .iter()
                .all(|difference| difference.expected);
            self.selections.push(SelectionEvidence {
                fact_index,
                proves: proves[fact_index].clone(),
                expected_classification: expectation.classification,
                expected_code: expectation.code.clone(),
                limitation_resolution: expectation.limitation_resolution.clone(),
                expectation_matched,
                go_oracle: GoOracleFactObservation {
                    occurrence,
                    complete: facts.complete,
                    recovered: facts.recovered,
                    truncated: facts.truncated,
                    type_view_states: facts.type_view_states.clone(),
                    actual: self
                        .snapshot
                        .graph()
                        .type_record(facts.actual())
                        .map_or(serde_json::Value::Null, oracle_record_value),
                },
                rust_candidate: candidate.clone(),
            });
        }
        self.mapping.failed_files = failed_files.len();
    }

    fn observe_expected_non_supported(
        &mut self,
        fact_index: usize,
        facts: &OccurrenceTypeFacts,
        candidate: &PrimitiveLiteralCandidate,
        expectation: &SelectionExpectation,
        observation: NonSupportedObservation<'_>,
    ) {
        let occurrence = facts.occurrence();
        if candidate.oxc_node_id.is_some() {
            self.mapping.mapped += 1;
        } else {
            self.mapping.unmapped += 1;
        }
        if observation.state_matches {
            self.differences.push(expected_difference(
                observation.category,
                expectation
                    .code
                    .as_deref()
                    .expect("non-supported expectations require a code"),
                Some((fact_index, occurrence)),
                "classification",
                json_value(&expectation.classification),
                json_value(candidate),
                observation.explanation,
            ));
        } else {
            self.differences.push(mismatch(
                DifferenceCategory::Transport,
                "classification-state-mismatch",
                Some((fact_index, occurrence)),
                "classification",
                json_value(expectation),
                json_value(candidate),
                "the Rust candidate state no longer matches its explicit fixture classification",
            ));
        }
    }

    fn validate_go_expectation(
        &mut self,
        fact_index: usize,
        facts: &OccurrenceTypeFacts,
        expectation: &GoOracleExpectation,
    ) {
        let actual_record = self.snapshot.graph().type_record(facts.actual());
        let fact_matches = facts.complete == expectation.complete
            && facts.recovered == expectation.recovered
            && facts.truncated == expectation.truncated
            && facts.type_view_states == expectation.type_view_states;
        let type_matches = actual_record.is_some_and(|record| {
            oracle_type_matches_expectation(
                self.snapshot.graph(),
                record,
                &expectation.actual,
                &mut BTreeSet::new(),
            )
        });
        if fact_matches && type_matches {
            return;
        }
        let occurrence = facts.occurrence();
        self.differences.push(mismatch(
            DifferenceCategory::Semantic,
            "go-oracle-expectation-mismatch",
            Some((fact_index, occurrence)),
            "goOracle",
            json_value(expectation),
            json_value(&serde_json::json!({
                "complete": facts.complete,
                "recovered": facts.recovered,
                "truncated": facts.truncated,
                "typeViewStates": facts.type_view_states,
                "actual": actual_record.map(oracle_record_value),
            })),
            "the checked Go oracle observation drifted from the fixture's structured expectation",
        ));
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
            self.differences.push(mismatch(
                DifferenceCategory::Semantic,
                "supported-go-type-outside-primitive-literal-slice",
                context,
                "roots[actual]",
                actual_record.map(oracle_record_value),
                candidate.roots.first().and_then(json_value),
                "a fixture classified as supported must have an in-category complete Go actual type",
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
        repeated_equal: bool,
        request: PinnedRequestEvidence,
    ) -> ConformanceCase {
        sort_differences(&mut self.differences);
        ConformanceCase {
            name,
            facts: self.snapshot.facts().len(),
            request,
            repeated_rust_output_equal: repeated_equal,
            classifications: self.classifications,
            selections: self.selections,
            go_oracle: GoOracleEvidence {
                typescript_version: self.snapshot.typescript_version.clone(),
                typescript_revision: self.snapshot.typescript_revision.clone(),
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

fn oracle_type_matches_expectation(
    graph: &TypeGraph,
    record: &crate::facts::TypeRecord,
    expectation: &GoOracleTypeExpectation,
    visiting: &mut BTreeSet<TypeId>,
) -> bool {
    if record.type_kind != expectation.type_kind
        || record.state != expectation.state
        || record.complete != expectation.complete
        || record.truncated != expectation.truncated
        || record.literal != expectation.literal
        || expectation
            .member_count
            .is_some_and(|count| record.members.len() != count)
    {
        return false;
    }
    let Some(expected_members) = &expectation.members else {
        return true;
    };
    if expected_members.len() != record.members.len() || !visiting.insert(record.id.clone()) {
        return false;
    }
    let matched = record
        .members
        .iter()
        .zip(expected_members)
        .all(|(member, expectation)| {
            graph.type_record(member).is_some_and(|record| {
                oracle_type_matches_expectation(graph, record, expectation, visiting)
            })
        });
    visiting.remove(&record.id);
    matched
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
        summary.accounted_selections += case.classifications.supported
            + case.classifications.unsupported
            + case.classifications.budget
            + case.classifications.mapping;
        summary.candidate_records += case.candidate_states.complete
            + case.candidate_states.truncated
            + case.candidate_states.unsupported
            + case.candidate_states.error;
        summary.supported_records += case.supported_records;
        summary.matched_supported_records += case.matched_supported_records;
        summary.classifications.merge(case.classifications);
        for difference in &case.differences {
            *summary
                .differences_by_category
                .entry(category_name(difference.category).to_owned())
                .or_default() += 1;
            if difference.expected {
                summary.expected_differences += 1;
            } else {
                *summary
                    .unexplained_differences_by_category
                    .entry(category_name(difference.category).to_owned())
                    .or_default() += 1;
                if matches!(
                    difference.category,
                    DifferenceCategory::Semantic
                        | DifferenceCategory::Transport
                        | DifferenceCategory::Mapping
                ) {
                    summary.blocking_differences += 1;
                }
            }
        }
    }
    summary.supported_compatibility_ppm =
        ratio_ppm(summary.matched_supported_records, summary.supported_records);
    summary.selection_accounting_ppm = ratio_ppm(summary.accounted_selections, summary.facts);
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

fn duration_nanoseconds(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn tool_version(command: &str, args: &[&str]) -> String {
    Command::new(command)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unavailable".to_owned())
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

fn validate_expectations(manifest: &CorpusManifest) -> Result<(), String> {
    for (index, selection) in manifest.selections.iter().enumerate() {
        let expectation = selection
            .conformance
            .as_ref()
            .expect("selected cases require every expectation");
        let requires_code = expectation.classification != ExpectedClassification::Supported;
        if requires_code
            != expectation
                .code
                .as_deref()
                .is_some_and(|code| !code.is_empty())
        {
            return Err(format!(
                "transport: case {:?} selections[{index}] must {} an explicit classification code",
                manifest.name,
                if requires_code { "provide" } else { "omit" }
            ));
        }
        let requires_limitation = matches!(
            expectation.classification,
            ExpectedClassification::Unsupported | ExpectedClassification::Mapping
        );
        if requires_limitation != expectation.limitation_resolution.is_some() {
            return Err(format!(
                "transport: case {:?} selections[{index}] must {} a stable limitation resolution",
                manifest.name,
                if requires_limitation {
                    "provide"
                } else {
                    "omit"
                }
            ));
        }
        if expectation
            .limitation_resolution
            .as_ref()
            .is_some_and(|resolution| {
                resolution.owner.trim().is_empty() || resolution.action.trim().is_empty()
            })
        {
            return Err(format!(
                "transport: case {:?} selections[{index}] limitation owner and action must be non-empty",
                manifest.name
            ));
        }
        if expectation.go_oracle.type_view_states.actual != TypeViewState::Available {
            return Err(format!(
                "transport: case {:?} selections[{index}] Go actual view must be available",
                manifest.name
            ));
        }
        validate_type_expectation(&manifest.name, index, &expectation.go_oracle.actual)?;
    }
    Ok(())
}

fn validate_type_expectation(
    case_name: &str,
    selection_index: usize,
    expectation: &GoOracleTypeExpectation,
) -> Result<(), String> {
    if let (Some(member_count), Some(members)) = (expectation.member_count, &expectation.members)
        && member_count != members.len()
    {
        return Err(format!(
            "transport: case {case_name:?} selections[{selection_index}] memberCount {member_count} does not match {} structured members",
            members.len()
        ));
    }
    if let Some(members) = &expectation.members {
        for member in members {
            validate_type_expectation(case_name, selection_index, member)?;
        }
    }
    Ok(())
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
    fn expected_mapping_gaps_are_accounted_without_blocking_supported_records() {
        let mut summary = ConformanceSummary {
            supported_records: 15,
            matched_supported_records: 15,
            supported_compatibility_ppm: 1_000_000,
            accounted_selections: 13,
            selection_accounting_ppm: 1_000_000,
            ..ConformanceSummary::default()
        };
        summary
            .differences_by_category
            .insert("mapping".to_owned(), 3);
        summary.classifications = ClassificationCoverage {
            supported: 10,
            mapping: 3,
            ..ClassificationCoverage::default()
        };

        assert!(threshold_passes(&summary, compatibility_threshold()));
        summary
            .unexplained_differences_by_category
            .insert("mapping".to_owned(), 1);
        assert!(!threshold_passes(&summary, compatibility_threshold()));
        summary
            .unexplained_differences_by_category
            .insert("mapping".to_owned(), 0);
        summary.selection_accounting_ppm = 999_999;
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

    #[test]
    fn exact_shadow_conformance_does_not_imply_authority_readiness() {
        let summary = ConformanceSummary {
            supported_records: 29,
            matched_supported_records: 29,
            supported_compatibility_ppm: 1_000_000,
            classifications: ClassificationCoverage {
                supported: 20,
                unsupported: 4,
                budget: 1,
                mapping: 3,
            },
            accounted_selections: 28,
            selection_accounting_ppm: 1_000_000,
            ..ConformanceSummary::default()
        };

        assert!(threshold_passes(&summary, compatibility_threshold()));
        let readiness = authority_readiness(&[]);
        assert!(!readiness.ready_for_later_authority_decision);
        assert_eq!(readiness.status, "not-ready");
        assert!(readiness.resolved_rollout_limitations.is_empty());
        assert_eq!(readiness.blockers.len(), 3);
    }
}

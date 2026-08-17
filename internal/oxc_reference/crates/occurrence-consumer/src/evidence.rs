use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::Instant,
};

use oxc_allocator::Allocator;
use serde::{Deserialize, Serialize};

use crate::{
    contract::{DiagnosticCode, KindCoverage, Metrics},
    facts::{EntityStateCounts, GraphRecordCounts, ProducerBudgetReport, SemanticSnapshot},
    inspector::{InspectionDiagnosticCode, InspectorLimits},
    oxc::OxcConsumer,
};

pub const EVIDENCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReport {
    pub schema_version: u32,
    pub evidence_kind: &'static str,
    pub comparison_scope: &'static str,
    pub environment: EnvironmentEvidence,
    pub inspector_limits: InspectorLimits,
    pub cases: Vec<CaseEvidence>,
    pub totals: ObservationEvidence,
    pub timing: TimingSummary,
    pub artifacts: ArtifactEvidence,
    pub compatibility_gate: CompatibilityGate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentEvidence {
    pub operating_system: &'static str,
    pub architecture: &'static str,
    pub rustc: String,
    pub go: String,
    pub command: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseEvidence {
    pub name: String,
    pub source_files: Vec<String>,
    pub first_pass: PassTiming,
    pub repeated_pass: PassTiming,
    pub repeated_observations_equal: bool,
    pub observation: ObservationEvidence,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassTiming {
    pub producer_nanoseconds: u64,
    pub consumer_nanoseconds: u64,
    pub total_nanoseconds: u64,
    pub snapshot_bytes: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservationEvidence {
    pub producer: ProducerEvidence,
    pub consumer: ConsumerEvidence,
    pub comparison: ComparisonEvidence,
    pub diagnostics: Vec<LayerDiagnostic>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerEvidence {
    pub typescript_version: String,
    pub typescript_revision: String,
    pub diagnostic_count: u32,
    pub budgets: ProducerBudgetReport,
    pub files: usize,
    pub facts: usize,
    pub records: GraphRecordCounts,
    pub entity_states: EntityStateCounts,
    pub shared_edge_references: usize,
    pub edge_references: usize,
    pub sharing_ratio_ppm: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerEvidence {
    pub source_files: usize,
    pub parsed_files: usize,
    pub failed_files: usize,
    pub mapping: Metrics,
    pub by_syntax_kind: Vec<KindCoverage>,
    pub attached_facts: usize,
    pub inspected_nodes: usize,
    pub inspected_edges: usize,
    pub inspector_budget_truncated_facts: usize,
    pub inspection_diagnostics: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonEvidence {
    pub go_oracle_facts: usize,
    pub identity_mapped_facts: usize,
    pub actual_roots_transport_preserved: usize,
    pub semantic_equivalence_claims: usize,
    pub differences: Vec<FactDifference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactDifference {
    pub fact_index: usize,
    pub file: String,
    pub syntax_kind: String,
    pub code: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FailureLayer {
    Protocol,
    Exporter,
    Mapping,
    Consumer,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerDiagnostic {
    pub layer: FailureLayer,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingSummary {
    pub first_pass_nanoseconds: u64,
    pub repeated_pass_nanoseconds: u64,
    pub case_count: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEvidence {
    pub consumer_executable_bytes: u64,
    pub peak_or_current_resident_bytes: Option<u64>,
    pub resident_measurement: String,
    pub total_snapshot_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityGate {
    pub first_candidate: &'static str,
    pub decision: &'static str,
    pub required_mapping_coverage_ppm: u64,
    pub required_ambiguity_count: usize,
    pub required_transport_mismatch_count: usize,
    pub go_authoritative: Vec<&'static str>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorpusManifest {
    name: String,
    project: String,
    capabilities: Vec<String>,
    #[serde(default)]
    budgets: ProducerBudgetReportRequest,
    selections: Vec<CorpusSelection>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProducerBudgetReportRequest {
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
    budgets: ProducerBudgetReportRequest,
    selections: Vec<ProducerSelection>,
}

#[derive(Clone, Debug, Serialize)]
struct ProducerSelection {
    file: String,
    start: usize,
    end: usize,
}

struct CompletedPass {
    timing: PassTiming,
    observation: ObservationEvidence,
}

pub fn run_evidence(
    tsfacts_binary: &Path,
    corpus_root: &Path,
    limits: InspectorLimits,
) -> Result<EvidenceReport, String> {
    let tsfacts_binary = tsfacts_binary
        .canonicalize()
        .map_err(|error| format!("exporter: resolve {}: {error}", tsfacts_binary.display()))?;
    let corpus_root = corpus_root
        .canonicalize()
        .map_err(|error| format!("consumer: resolve {}: {error}", corpus_root.display()))?;
    let case_directories = sorted_case_directories(&corpus_root)?;
    let mut cases = Vec::with_capacity(case_directories.len());

    for case_directory in case_directories {
        let manifest = read_manifest(&case_directory)?;
        let request = build_request(&case_directory, &manifest)?;
        let source_files = request
            .selections
            .iter()
            .map(|selection| selection.file.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let first = run_pass(
            &tsfacts_binary,
            &case_directory,
            &request,
            &source_files,
            limits,
        )?;
        let repeated = run_pass(
            &tsfacts_binary,
            &case_directory,
            &request,
            &source_files,
            limits,
        )?;
        let observations_equal = first.observation == repeated.observation;
        let mut observation = repeated.observation;
        if !observations_equal {
            observation.diagnostics.push(LayerDiagnostic {
                layer: FailureLayer::Protocol,
                code: "non-deterministic-observation".to_owned(),
                fact_index: None,
                file: None,
                message: "first and repeated one-shot observations differ".to_owned(),
            });
        }
        cases.push(CaseEvidence {
            name: manifest.name,
            source_files,
            first_pass: first.timing,
            repeated_pass: repeated.timing,
            repeated_observations_equal: observations_equal,
            observation,
        });
    }

    let totals = aggregate_observations(cases.iter().map(|case| &case.observation));
    let timing = TimingSummary {
        first_pass_nanoseconds: cases
            .iter()
            .map(|case| case.first_pass.total_nanoseconds)
            .sum(),
        repeated_pass_nanoseconds: cases
            .iter()
            .map(|case| case.repeated_pass.total_nanoseconds)
            .sum(),
        case_count: cases.len(),
    };
    let total_snapshot_bytes = cases
        .iter()
        .map(|case| case.first_pass.snapshot_bytes + case.repeated_pass.snapshot_bytes)
        .sum();
    let (resident_bytes, resident_measurement) = resident_memory();
    let consumer_executable_bytes = std::env::current_exe()
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .map_or(0, |metadata| metadata.len());

    Ok(EvidenceReport {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        evidence_kind: "ts7-go-to-internal-oxc-rust-spike",
        comparison_scope: "wire-identity-attachment-and-bounded-inspection-only",
        environment: EnvironmentEvidence {
            operating_system: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            rustc: tool_version("rustc", &["--version"]),
            go: tool_version("go", &["version"]),
            command: "./internal/oxc_reference/run-evidence.sh --output <path>",
        },
        inspector_limits: limits,
        cases,
        totals,
        timing,
        artifacts: ArtifactEvidence {
            consumer_executable_bytes,
            peak_or_current_resident_bytes: resident_bytes,
            resident_measurement,
            total_snapshot_bytes,
        },
        compatibility_gate: CompatibilityGate {
            first_candidate: "schema-v1 occurrence identity and attachment plumbing",
            decision: "safe-to-mechanically-port-behind-go-oracle; not-safe-to-replace-go-semantics",
            required_mapping_coverage_ppm: 1_000_000,
            required_ambiguity_count: 0,
            required_transport_mismatch_count: 0,
            go_authoritative: vec![
                "project loading and module resolution",
                "binding and symbol identity",
                "type construction, inference, contextual typing, and widening",
                "overload and generic instantiation",
                "control-flow narrowing",
                "semantic entity completeness and recovery",
            ],
        },
    })
}

fn run_pass(
    tsfacts_binary: &Path,
    case_directory: &Path,
    request: &ProducerRequest<'_>,
    source_files: &[String],
    limits: InspectorLimits,
) -> Result<CompletedPass, String> {
    let request_json = serde_json::to_vec(request)
        .map_err(|error| format!("protocol: encode producer request: {error}"))?;
    let producer_started = Instant::now();
    let mut child = Command::new(tsfacts_binary)
        .current_dir(case_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("exporter: launch {}: {error}", tsfacts_binary.display()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "exporter: producer stdin is unavailable".to_owned())?
        .write_all(&request_json)
        .map_err(|error| format!("exporter: write producer request: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("exporter: wait for producer: {error}"))?;
    let producer_nanoseconds = duration_nanoseconds(producer_started.elapsed().as_nanos());
    if !output.status.success() {
        return Err(format!(
            "exporter: tsfacts exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let consumer_started = Instant::now();
    let snapshot = Arc::new(
        SemanticSnapshot::from_json_lines(BufReader::new(output.stdout.as_slice()))
            .map_err(|error| format!("protocol: decode producer output: {error}"))?,
    );
    let observation = observe_snapshot(case_directory, source_files, snapshot, limits)?;
    let consumer_nanoseconds = duration_nanoseconds(consumer_started.elapsed().as_nanos());

    Ok(CompletedPass {
        timing: PassTiming {
            producer_nanoseconds,
            consumer_nanoseconds,
            total_nanoseconds: producer_nanoseconds.saturating_add(consumer_nanoseconds),
            snapshot_bytes: output.stdout.len(),
        },
        observation,
    })
}

fn observe_snapshot(
    case_directory: &Path,
    source_files: &[String],
    snapshot: Arc<SemanticSnapshot>,
    limits: InspectorLimits,
) -> Result<ObservationEvidence, String> {
    let graph = snapshot.graph();
    let records = graph.record_counts();
    let entity_states = graph.state_counts();
    let (shared_edge_references, edge_references) = graph.sharing_counts();
    let mut observation = ObservationEvidence {
        producer: ProducerEvidence {
            typescript_version: snapshot.typescript_version.clone(),
            typescript_revision: snapshot.typescript_revision.clone(),
            diagnostic_count: snapshot.diagnostic_count,
            budgets: snapshot.budgets,
            files: snapshot.file_count(),
            facts: snapshot.facts().len(),
            records,
            entity_states,
            shared_edge_references,
            edge_references,
            sharing_ratio_ppm: ratio_ppm(shared_edge_references, edge_references),
        },
        comparison: ComparisonEvidence {
            go_oracle_facts: snapshot.facts().len(),
            ..ComparisonEvidence::default()
        },
        ..ObservationEvidence::default()
    };
    observation.consumer.source_files = source_files.len();

    for file in source_files {
        let source_path = case_directory.join(file);
        let source = fs::read_to_string(&source_path)
            .map_err(|error| format!("consumer: read {}: {error}", source_path.display()))?;
        let allocator = Allocator::default();
        let mut consumer = match OxcConsumer::parse(&allocator, file, &source) {
            Ok(consumer) => consumer,
            Err(error) => {
                observation.consumer.failed_files += 1;
                observation.diagnostics.push(LayerDiagnostic {
                    layer: FailureLayer::Consumer,
                    code: "oxc-parse-or-semantic-error".to_owned(),
                    fact_index: None,
                    file: Some(file.clone()),
                    message: error,
                });
                for (fact_index, fact) in snapshot
                    .facts()
                    .iter()
                    .enumerate()
                    .filter(|(_, fact)| fact.file == *file)
                {
                    observation.comparison.differences.push(FactDifference {
                        fact_index,
                        file: fact.file.clone(),
                        syntax_kind: fact.syntax_kind.clone(),
                        code: "consumer-file-failed".to_owned(),
                    });
                }
                continue;
            }
        };
        observation.consumer.parsed_files += 1;
        let report = consumer
            .attach_file(Arc::clone(&snapshot))
            .map_err(|error| format!("consumer: attach {file}: {error}"))?;
        add_metrics(&mut observation.consumer.mapping, &report.summary);
        merge_kind_coverage(
            &mut observation.consumer.by_syntax_kind,
            &report.by_syntax_kind,
        );

        for diagnostic in &report.diagnostics {
            let code = match diagnostic.code {
                DiagnosticCode::Unmapped => "unmapped",
                DiagnosticCode::MultiplyMapped => "multiply-mapped",
            };
            observation.diagnostics.push(LayerDiagnostic {
                layer: FailureLayer::Mapping,
                code: code.to_owned(),
                fact_index: Some(diagnostic.fact_index),
                file: Some(diagnostic.file.clone()),
                message: format!(
                    "{} candidate(s) for {}:{}-{} {}",
                    diagnostic.candidates.len(),
                    diagnostic.file,
                    diagnostic.span.start,
                    diagnostic.span.end,
                    diagnostic.syntax_kind
                ),
            });
            let fact = &snapshot.facts()[diagnostic.fact_index];
            observation.comparison.differences.push(FactDifference {
                fact_index: diagnostic.fact_index,
                file: fact.file.clone(),
                syntax_kind: fact.syntax_kind.clone(),
                code: code.to_owned(),
            });
        }

        for mapping in &report.mappings {
            observation.comparison.identity_mapped_facts += 1;
            let node_id = consumer
                .node_for_fact(mapping.fact_index)
                .ok_or_else(|| format!("consumer: fact {} lost its NodeId", mapping.fact_index))?;
            let attached = consumer
                .type_facts_for_node(node_id)
                .find(|facts| facts.fact_index == mapping.fact_index)
                .ok_or_else(|| {
                    format!("consumer: fact {} lost its attachment", mapping.fact_index)
                })?;
            observation.consumer.attached_facts += 1;
            let inspection = attached.inspect(limits);
            observation.consumer.inspected_nodes += inspection.summary.nodes;
            observation.consumer.inspected_edges += inspection.summary.edges;
            if inspection.summary.truncated {
                observation.consumer.inspector_budget_truncated_facts += 1;
            }
            for diagnostic in &inspection.diagnostics {
                *observation
                    .consumer
                    .inspection_diagnostics
                    .entry(wire_name(diagnostic.code))
                    .or_default() += 1;
            }
            let actual_root = inspection
                .roots
                .first()
                .and_then(|root| root.type_id.as_deref());
            if actual_root == Some(attached.facts.actual_type.as_str()) {
                observation.comparison.actual_roots_transport_preserved += 1;
            } else {
                observation.comparison.differences.push(FactDifference {
                    fact_index: mapping.fact_index,
                    file: attached.facts.file.clone(),
                    syntax_kind: attached.facts.syntax_kind.clone(),
                    code: "actual-root-transport-mismatch".to_owned(),
                });
            }
        }
    }

    observation.diagnostics.sort();
    observation.comparison.differences.sort_by(|left, right| {
        left.fact_index
            .cmp(&right.fact_index)
            .then_with(|| left.code.cmp(&right.code))
    });
    Ok(observation)
}

fn sorted_case_directories(corpus_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut directories = fs::read_dir(corpus_root)
        .map_err(|error| format!("consumer: read {}: {error}", corpus_root.display()))?
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
        fs::read(&path).map_err(|error| format!("protocol: read {}: {error}", path.display()))?;
    serde_json::from_slice(&source)
        .map_err(|error| format!("protocol: decode {}: {error}", path.display()))
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
                    .map_err(|error| format!("protocol: read {}: {error}", path.display()))?;
                source_cache.entry(selection.file.clone()).or_insert(source)
            }
        };
        let start =
            nth_occurrence(source, &selection.text, selection.occurrence).ok_or_else(|| {
                format!(
                    "protocol: selection {:?} occurrence {} is absent from {}",
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

fn nth_occurrence(source: &str, text: &str, occurrence: usize) -> Option<usize> {
    source
        .match_indices(text)
        .nth(occurrence)
        .map(|(start, _)| start)
}

fn aggregate_observations<'a>(
    observations: impl Iterator<Item = &'a ObservationEvidence>,
) -> ObservationEvidence {
    let mut total = ObservationEvidence::default();
    for observation in observations {
        if total.producer.typescript_version.is_empty() {
            total.producer.typescript_version = observation.producer.typescript_version.clone();
            total.producer.typescript_revision = observation.producer.typescript_revision.clone();
        }
        total.producer.diagnostic_count += observation.producer.diagnostic_count;
        total.producer.budgets.limits.max_type_nodes = total
            .producer
            .budgets
            .limits
            .max_type_nodes
            .max(observation.producer.budgets.limits.max_type_nodes);
        total.producer.budgets.limits.max_type_depth = total
            .producer
            .budgets
            .limits
            .max_type_depth
            .max(observation.producer.budgets.limits.max_type_depth);
        total.producer.budgets.type_nodes_used += observation.producer.budgets.type_nodes_used;
        total.producer.budgets.max_type_depth_observed = total
            .producer
            .budgets
            .max_type_depth_observed
            .max(observation.producer.budgets.max_type_depth_observed);
        total.producer.budgets.truncated |= observation.producer.budgets.truncated;
        total.producer.files += observation.producer.files;
        total.producer.facts += observation.producer.facts;
        total.producer.records.types += observation.producer.records.types;
        total.producer.records.declarations += observation.producer.records.declarations;
        total.producer.records.symbols += observation.producer.records.symbols;
        total.producer.records.signatures += observation.producer.records.signatures;
        total.producer.records.edges += observation.producer.records.edges;
        total.producer.entity_states.complete += observation.producer.entity_states.complete;
        total.producer.entity_states.truncated += observation.producer.entity_states.truncated;
        total.producer.entity_states.unsupported += observation.producer.entity_states.unsupported;
        total.producer.entity_states.error += observation.producer.entity_states.error;
        total.producer.shared_edge_references += observation.producer.shared_edge_references;
        total.producer.edge_references += observation.producer.edge_references;
        total.consumer.source_files += observation.consumer.source_files;
        total.consumer.parsed_files += observation.consumer.parsed_files;
        total.consumer.failed_files += observation.consumer.failed_files;
        add_metrics(&mut total.consumer.mapping, &observation.consumer.mapping);
        merge_kind_coverage(
            &mut total.consumer.by_syntax_kind,
            &observation.consumer.by_syntax_kind,
        );
        total.consumer.attached_facts += observation.consumer.attached_facts;
        total.consumer.inspected_nodes += observation.consumer.inspected_nodes;
        total.consumer.inspected_edges += observation.consumer.inspected_edges;
        total.consumer.inspector_budget_truncated_facts +=
            observation.consumer.inspector_budget_truncated_facts;
        for (code, count) in &observation.consumer.inspection_diagnostics {
            *total
                .consumer
                .inspection_diagnostics
                .entry(code.clone())
                .or_default() += count;
        }
        total.comparison.go_oracle_facts += observation.comparison.go_oracle_facts;
        total.comparison.identity_mapped_facts += observation.comparison.identity_mapped_facts;
        total.comparison.actual_roots_transport_preserved +=
            observation.comparison.actual_roots_transport_preserved;
        total
            .comparison
            .differences
            .extend(observation.comparison.differences.clone());
        total.diagnostics.extend(observation.diagnostics.clone());
    }
    total.producer.sharing_ratio_ppm = ratio_ppm(
        total.producer.shared_edge_references,
        total.producer.edge_references,
    );
    total
}

fn add_metrics(total: &mut Metrics, metrics: &Metrics) {
    total.facts += metrics.facts;
    total.mapped += metrics.mapped;
    total.exact += metrics.exact;
    total.normalized += metrics.normalized;
    total.unmapped += metrics.unmapped;
    total.multiply_mapped += metrics.multiply_mapped;
}

fn merge_kind_coverage(total: &mut Vec<KindCoverage>, additions: &[KindCoverage]) {
    let mut coverage = total
        .drain(..)
        .map(|item| (item.syntax_kind, item.metrics))
        .collect::<BTreeMap<_, _>>();
    for item in additions {
        add_metrics(
            coverage.entry(item.syntax_kind.clone()).or_default(),
            &item.metrics,
        );
    }
    *total = coverage
        .into_iter()
        .map(|(syntax_kind, metrics)| KindCoverage {
            syntax_kind,
            metrics,
        })
        .collect();
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

fn wire_name(value: InspectionDiagnosticCode) -> String {
    serde_json::to_value(value)
        .expect("serialize closed inspection diagnostic code")
        .as_str()
        .expect("inspection diagnostic code serializes as a string")
        .to_owned()
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

fn resident_memory() -> (Option<u64>, String) {
    platform_resident_memory()
}

#[cfg(target_os = "linux")]
fn platform_resident_memory() -> (Option<u64>, String) {
    let bytes = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            })
        })
        .map(|kilobytes| kilobytes.saturating_mul(1024));
    (bytes, "linux-proc-vmhwm-peak".to_owned())
}

#[cfg(target_os = "macos")]
fn platform_resident_memory() -> (Option<u64>, String) {
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Timeval {
        seconds: i64,
        microseconds: i32,
        padding: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rusage {
        user_time: Timeval,
        system_time: Timeval,
        max_resident_bytes: i64,
        remaining_fields: [i64; 13],
    }

    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }

    let mut usage = Rusage::default();
    // macOS defines RUSAGE_SELF as zero and reports ru_maxrss in bytes.
    let result = unsafe { getrusage(0, &raw mut usage) };
    let bytes = (result == 0)
        .then(|| u64::try_from(usage.max_resident_bytes).ok())
        .flatten();
    (bytes, "macos-getrusage-peak".to_owned())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_resident_memory() -> (Option<u64>, String) {
    (None, "unavailable".to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use super::*;

    #[test]
    fn occurrence_offsets_are_utf8_bytes_and_zero_based() {
        let source = "const café = 'x'; café;";
        assert_eq!(nth_occurrence(source, "café", 0), Some(6));
        assert_eq!(nth_occurrence(source, "café", 1), Some(19));
        assert_eq!(nth_occurrence(source, "missing", 0), None);
    }

    #[test]
    fn kind_coverage_is_sorted_and_additive() {
        let mut total = vec![KindCoverage {
            syntax_kind: "KindStringLiteral".to_owned(),
            metrics: Metrics {
                facts: 1,
                mapped: 1,
                exact: 1,
                ..Metrics::default()
            },
        }];
        merge_kind_coverage(
            &mut total,
            &[
                KindCoverage {
                    syntax_kind: "KindIdentifier".to_owned(),
                    metrics: Metrics {
                        facts: 2,
                        mapped: 1,
                        unmapped: 1,
                        ..Metrics::default()
                    },
                },
                KindCoverage {
                    syntax_kind: "KindStringLiteral".to_owned(),
                    metrics: Metrics {
                        facts: 1,
                        mapped: 1,
                        normalized: 1,
                        ..Metrics::default()
                    },
                },
            ],
        );
        assert_eq!(total[0].syntax_kind, "KindIdentifier");
        assert_eq!(total[1].metrics.facts, 2);
        assert_eq!(total[1].metrics.mapped, 2);
    }

    #[test]
    fn evidence_keeps_mapping_and_entity_state_diagnostics_distinct() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("internal/tsfacts/testdata/canonical/v0");
        let fixture = fs::File::open(fixture_root.join("evidence-diagnostics.jsonl"))
            .expect("open shared evidence fixture");
        let snapshot = Arc::new(
            SemanticSnapshot::from_json_lines(BufReader::new(fixture))
                .expect("decode shared evidence fixture"),
        );
        let observation = observe_snapshot(
            &fixture_root,
            &["src/evidence-diagnostics.ts".to_owned()],
            snapshot,
            InspectorLimits::default(),
        )
        .expect("observe shared evidence fixture");

        assert_eq!(observation.consumer.mapping.facts, 2);
        assert_eq!(observation.consumer.mapping.mapped, 1);
        assert_eq!(observation.consumer.mapping.unmapped, 1);
        assert_eq!(observation.producer.entity_states.unsupported, 1);
        assert_eq!(observation.producer.entity_states.truncated, 1);
        assert_eq!(
            observation
                .consumer
                .inspection_diagnostics
                .get("entity-unsupported"),
            Some(&1)
        );
        assert!(observation.diagnostics.iter().any(|diagnostic| {
            diagnostic.layer == FailureLayer::Mapping && diagnostic.code == "unmapped"
        }));
        assert_eq!(observation.comparison.differences[0].code, "unmapped");
    }
}

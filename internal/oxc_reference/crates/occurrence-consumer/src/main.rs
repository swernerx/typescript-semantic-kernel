use std::{collections::BTreeMap, fs, io::BufReader, sync::Arc};

use oxc_allocator::Allocator;
use oxc_occurrence_consumer::{
    contract::{Report, correlate},
    facts::SemanticSnapshot,
    fixture::load_fixtures,
    inspector::{InspectionReport, InspectorLimits},
    oxc::OxcConsumer,
};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FixtureReport {
    fixture: String,
    description: String,
    contract_report: Report,
    files: Vec<FileReport>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileReport {
    file: String,
    oxc_node_count: usize,
    projected_candidate_count: usize,
    report: oxc_occurrence_consumer::contract::Report,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotInspection {
    mapping_report: Report,
    attachments: Vec<AttachmentInspection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentInspection {
    fact_index: usize,
    node_id: usize,
    inspection: InspectionReport,
}

fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command] if command == "fixtures" => print_fixture_reports(),
        [command, snapshot, source] if command == "inspect" => {
            inspect_snapshot(snapshot, source, None)
        }
        [command, snapshot, source, logical_file] if command == "inspect" => {
            inspect_snapshot(snapshot, source, Some(logical_file.as_str()))
        }
        _ => Err(
            "usage: oxc-occurrence-map fixtures | inspect <snapshot.jsonl> <source> [logical-file]"
                .to_owned(),
        ),
    }
}

fn print_fixture_reports() -> Result<(), String> {
    let mut output = Vec::new();
    for (path, fixture) in load_fixtures()? {
        let contract_report = correlate(&fixture.facts, &fixture.nodes)?;
        let facts_by_file = fixture
            .facts
            .iter()
            .fold(BTreeMap::new(), |mut files, fact| {
                files
                    .entry(&fact.file)
                    .or_insert_with(Vec::new)
                    .push(fact.clone());
                files
            });
        let mut files = Vec::new();
        for (file, source) in &fixture.sources {
            let allocator = Allocator::default();
            let mut consumer = OxcConsumer::parse(&allocator, file, source)?;
            let report =
                consumer.correlate(&facts_by_file.get(file).cloned().unwrap_or_default())?;
            files.push(FileReport {
                file: file.clone(),
                oxc_node_count: consumer.node_count(),
                projected_candidate_count: consumer.candidates().len(),
                report,
            });
        }
        output.push(FixtureReport {
            fixture: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>")
                .to_owned(),
            description: fixture.description,
            contract_report,
            files,
        });
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&output)
            .map_err(|error| format!("serialize fixture reports: {error}"))?
    );
    Ok(())
}

fn inspect_snapshot(
    snapshot_path: &str,
    source_path: &str,
    logical_file: Option<&str>,
) -> Result<(), String> {
    let snapshot_file = fs::File::open(snapshot_path)
        .map_err(|error| format!("open {snapshot_path:?}: {error}"))?;
    let snapshot = Arc::new(SemanticSnapshot::from_json_lines(BufReader::new(
        snapshot_file,
    ))?);
    let source = fs::read_to_string(source_path)
        .map_err(|error| format!("read {source_path:?}: {error}"))?;
    let file = logical_file.map_or_else(
        || {
            snapshot
                .facts()
                .first()
                .map(|fact| fact.file.as_str())
                .ok_or_else(|| "snapshot has no facts; pass a snapshot with occurrences".to_owned())
        },
        Ok,
    )?;
    if snapshot.facts().iter().any(|fact| fact.file != file) {
        return Err(format!(
            "snapshot facts span multiple files; select a single-file snapshot for {file:?}"
        ));
    }

    let allocator = Allocator::default();
    let mut consumer = OxcConsumer::parse(&allocator, file, &source)?;
    let mapping_report = consumer.attach(snapshot)?;
    let mut attachments = Vec::new();
    for mapping in &mapping_report.mappings {
        let node_id = consumer
            .node_for_fact(mapping.fact_index)
            .ok_or_else(|| format!("fact {} lost its mapped NodeId", mapping.fact_index))?;
        let attached = consumer
            .type_facts_for_node(node_id)
            .find(|attached| attached.fact_index == mapping.fact_index)
            .ok_or_else(|| format!("fact {} lost its TypeFacts attachment", mapping.fact_index))?;
        attachments.push(AttachmentInspection {
            fact_index: mapping.fact_index,
            node_id: node_id.index(),
            inspection: attached.inspect(InspectorLimits::default()),
        });
    }
    let output = SnapshotInspection {
        mapping_report,
        attachments,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output)
            .map_err(|error| format!("serialize inspection: {error}"))?
    );
    Ok(())
}

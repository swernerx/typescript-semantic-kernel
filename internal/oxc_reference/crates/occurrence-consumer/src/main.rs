use std::collections::BTreeMap;

use oxc_allocator::Allocator;
use oxc_occurrence_consumer::{
    contract::{Report, correlate},
    fixture::load_fixtures,
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

fn main() -> Result<(), String> {
    if std::env::args().nth(1).as_deref() != Some("fixtures") {
        return Err("usage: oxc-occurrence-map fixtures".to_owned());
    }

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

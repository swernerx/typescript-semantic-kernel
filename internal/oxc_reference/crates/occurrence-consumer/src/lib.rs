pub mod contract;
pub mod fixture;
pub mod oxc;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use oxc_allocator::Allocator;

    use crate::{
        contract::{DiagnosticCode, correlate},
        fixture::load_fixtures,
        oxc::OxcConsumer,
    };

    #[test]
    fn shared_contract_fixtures_match_expected_reports() {
        for (path, fixture) in load_fixtures().expect("load shared fixtures") {
            let actual = correlate(&fixture.facts, &fixture.nodes)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(
                serde_json::to_value(actual).expect("serialize report"),
                fixture.expected,
                "{}: {}",
                path.display(),
                fixture.description
            );
        }
    }

    #[test]
    fn oxc_traversal_correlates_shared_fixture_sources() {
        for (path, fixture) in load_fixtures().expect("load shared fixtures") {
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
            for (file, source) in &fixture.sources {
                let allocator = Allocator::default();
                let mut consumer = OxcConsumer::parse(&allocator, file, source)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                let facts = facts_by_file.get(file).cloned().unwrap_or_default();
                let report = consumer
                    .correlate(&facts)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));

                assert!(
                    consumer.node_count() > 0,
                    "{}: no OXC nodes",
                    path.display()
                );
                assert_eq!(consumer.source(), source);
                assert_eq!(report.summary.facts as usize, facts.len());
                assert_eq!(
                    report.summary.mapped,
                    report.summary.facts,
                    "{}: representative OXC projection coverage regressed: {report:?}",
                    path.display()
                );
                assert!(
                    report
                        .diagnostics
                        .iter()
                        .all(|diagnostic| diagnostic.code != DiagnosticCode::MultiplyMapped),
                    "{}: OXC projection must not fabricate ambiguity: {report:?}",
                    path.display()
                );
                for mapping in &report.mappings {
                    assert_eq!(
                        consumer
                            .node_for_fact(mapping.fact_index)
                            .map(|id| id.index()),
                        Some(mapping.node_id as usize),
                        "{}: portable mapping lost its owned OXC NodeId",
                        path.display()
                    );
                }
            }
        }
    }
}

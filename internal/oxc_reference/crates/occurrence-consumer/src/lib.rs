pub mod candidate;
pub mod contract;
pub mod evidence;
pub mod facts;
pub mod fixture;
pub mod inspector;
pub mod oxc;

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        io::{BufReader, Cursor},
        path::{Path, PathBuf},
        sync::Arc,
    };

    use oxc_allocator::Allocator;

    use crate::{
        candidate::{
            CandidateReason, CandidateSemantic, CandidateState, LiteralKind, NullLikeKind,
            PrimitiveKind,
        },
        contract::{DiagnosticCode, correlate},
        facts::{GraphRef, SemanticSnapshot, TypeViewState},
        fixture::load_fixtures,
        inspector::{InspectionDiagnosticCode, InspectorLimits},
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

    #[test]
    fn attaches_all_type_views_to_distinct_oxc_nodes_with_one_shared_graph() {
        let snapshot = load_canonical_snapshot("graph-inspector.jsonl");
        let source = fs::read_to_string(canonical_fixture_path("graph-inspector.ts"))
            .expect("read graph inspector source");
        let allocator = Allocator::default();
        let mut consumer = OxcConsumer::parse(&allocator, "src/graph-inspector.ts", &source)
            .expect("parse graph inspector source");

        let report = consumer.attach(snapshot).expect("attach semantic snapshot");
        assert_eq!(report.summary.facts, 2);
        assert_eq!(report.summary.mapped, 2);

        let first_node = consumer.node_for_fact(0).expect("first mapped node");
        let second_node = consumer.node_for_fact(1).expect("second mapped node");
        assert_ne!(first_node, second_node);
        let first = consumer
            .type_facts_for_node(first_node)
            .next()
            .expect("first node facts");
        let second = consumer
            .type_facts_for_node(second_node)
            .next()
            .expect("second node facts");

        assert_eq!(first.facts.actual().as_str(), "type:1");
        assert_eq!(
            first.facts.contextual().map(|id| id.as_str()),
            Some("type:6")
        );
        assert_eq!(first.facts.widened().map(|id| id.as_str()), Some("type:3"));
        assert_eq!(first.facts.apparent().map(|id| id.as_str()), Some("type:7"));
        assert_eq!(first.facts.declared().map(|id| id.as_str()), Some("type:8"));
        assert_eq!(second.facts.actual().as_str(), "type:1");
        assert_eq!(
            second.facts.type_view_states.contextual,
            TypeViewState::Unavailable
        );
        assert!(std::ptr::eq(first.graph(), second.graph()));
    }

    #[test]
    fn primitive_literal_candidate_is_structured_deterministic_and_state_preserving() {
        let snapshot = load_canonical_snapshot("primitive-literal-candidate.jsonl");
        let source = fs::read_to_string(canonical_fixture_path("primitive-literal-candidate.ts"))
            .expect("read primitive/literal candidate source");
        let allocator = Allocator::default();
        let mut consumer =
            OxcConsumer::parse(&allocator, "src/primitive-literal-candidate.ts", &source)
                .expect("parse primitive/literal candidate source");
        let report = consumer
            .attach(snapshot)
            .expect("attach primitive/literal candidate fixture");
        assert_eq!(report.summary.facts, 4);
        assert_eq!(report.summary.mapped, 4);

        let inspections = report
            .mappings
            .iter()
            .map(|mapping| {
                let node = consumer
                    .node_for_fact(mapping.fact_index)
                    .expect("mapped candidate node");
                let attached = consumer
                    .type_facts_for_node(node)
                    .find(|facts| facts.fact_index == mapping.fact_index)
                    .expect("attached candidate facts");
                let first = attached.inspect(InspectorLimits::default());
                let second = attached.inspect(InspectorLimits::default());
                assert_eq!(
                    serde_json::to_string(&first.primitive_literal_candidate)
                        .expect("serialize first candidate"),
                    serde_json::to_string(&second.primitive_literal_candidate)
                        .expect("serialize second candidate")
                );
                first
            })
            .collect::<Vec<_>>();

        let primitives = &inspections[0].primitive_literal_candidate;
        assert_eq!(primitives.roots.len(), 5);
        assert!(matches!(
            &candidate_type(primitives, "type:1").semantic,
            Some(CandidateSemantic::Primitive {
                primitive: PrimitiveKind::Boolean
            })
        ));
        assert!(matches!(
            &candidate_type(primitives, "type:2").semantic,
            Some(CandidateSemantic::Primitive {
                primitive: PrimitiveKind::String
            })
        ));
        assert!(matches!(
            &candidate_type(primitives, "type:3").semantic,
            Some(CandidateSemantic::Primitive {
                primitive: PrimitiveKind::Number
            })
        ));
        assert!(matches!(
            &candidate_type(primitives, "type:4").semantic,
            Some(CandidateSemantic::Primitive {
                primitive: PrimitiveKind::Bigint
            })
        ));
        assert!(matches!(
            &candidate_type(primitives, "type:5").semantic,
            Some(CandidateSemantic::NullLike {
                null_like: NullLikeKind::Null
            })
        ));

        let literals = &inspections[1].primitive_literal_candidate;
        for (id, kind, value) in [
            ("type:6", LiteralKind::String, "ready"),
            ("type:7", LiteralKind::Boolean, "true"),
            ("type:8", LiteralKind::Number, "42"),
            ("type:9", LiteralKind::Bigint, "42"),
        ] {
            assert!(matches!(
                &candidate_type(literals, id).semantic,
                Some(CandidateSemantic::Literal {
                    literal,
                    value: actual
                }) if *literal == kind && actual == value
            ));
        }
        assert!(matches!(
            &candidate_type(literals, "type:10").semantic,
            Some(CandidateSemantic::NullLike {
                null_like: NullLikeKind::Undefined
            })
        ));

        let union = &inspections[2].primitive_literal_candidate;
        assert!(matches!(
            &candidate_type(union, "type:11").semantic,
            Some(CandidateSemantic::Union { members })
                if members.iter().map(|id| id.as_str()).collect::<Vec<_>>()
                    == vec!["type:6", "type:7", "type:8", "type:9", "type:5", "type:10"]
        ));
        assert_eq!(union.summary.complete, 7);

        let incomplete = &inspections[3].primitive_literal_candidate;
        assert_eq!(incomplete.roots.len(), 5);
        assert_eq!(
            incomplete.roots[3].type_id.as_ref().map(|id| id.as_str()),
            Some("type:12")
        );
        assert_eq!(incomplete.roots[4].state, TypeViewState::Unavailable);
        assert!(incomplete.roots[4].type_id.is_none());
        assert_eq!(
            candidate_type(incomplete, "type:12").candidate_state,
            CandidateState::Unsupported
        );
        assert_eq!(
            candidate_type(incomplete, "type:13").candidate_state,
            CandidateState::Truncated
        );
        assert_eq!(
            candidate_type(incomplete, "type:14").candidate_state,
            CandidateState::Truncated
        );
        assert!(
            candidate_type(incomplete, "type:12")
                .reasons
                .contains(&CandidateReason::SourceUnsupported)
        );
        assert!(
            candidate_type(incomplete, "type:14")
                .reasons
                .contains(&CandidateReason::SourceTruncated)
        );
        assert_eq!(incomplete.summary.complete, 1);
        assert_eq!(incomplete.summary.truncated, 2);
        assert_eq!(incomplete.summary.unsupported, 1);

        let serialized = serde_json::to_string(
            &inspections
                .iter()
                .map(|inspection| &inspection.primitive_literal_candidate)
                .collect::<Vec<_>>(),
        )
        .expect("serialize structured primitive/literal candidates");
        assert!(!serialized.contains("\"display\""));
    }

    #[test]
    fn inspector_is_deterministic_identity_preserving_and_complete_about_states() {
        let snapshot = load_canonical_snapshot("graph-inspector.jsonl");
        let facts = &snapshot.facts()[0];
        let limits = InspectorLimits {
            max_depth: 16,
            max_nodes: 64,
            max_edges: 128,
        };
        let first = crate::inspector::GraphInspector::new(snapshot.graph(), limits).inspect(facts);
        let second = crate::inspector::GraphInspector::new(snapshot.graph(), limits).inspect(facts);
        assert_eq!(
            serde_json::to_string(&first).expect("serialize first inspection"),
            serde_json::to_string(&second).expect("serialize second inspection")
        );

        let identities = first
            .nodes
            .iter()
            .map(|node| &node.reference)
            .collect::<BTreeSet<_>>();
        assert_eq!(identities.len(), first.nodes.len());
        assert!(
            first
                .nodes
                .iter()
                .any(|node| node.category == "type:string")
        );
        assert!(
            first
                .nodes
                .iter()
                .any(|node| node.category == "type:reference")
        );
        assert!(
            first
                .nodes
                .iter()
                .any(|node| node.category == "type:object")
        );
        assert!(
            first
                .nodes
                .iter()
                .any(|node| node.category == "signature:call")
        );
        assert!(
            first
                .nodes
                .iter()
                .any(|node| node.category == "type:type_parameter")
        );
        assert!(
            first
                .nodes
                .iter()
                .any(|node| node.category == "type:conditional")
        );
        assert!(
            first.edges.iter().any(|edge| {
                edge.to == GraphRef::Type(crate::facts::TypeId("type:1".to_owned()))
            })
        );
        assert!(
            first
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == InspectionDiagnosticCode::EntityTruncated })
        );
        assert!(
            first.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == InspectionDiagnosticCode::EntityUnsupported
            })
        );
        assert!(!first.summary.truncated);
    }

    #[test]
    fn inspector_bounds_cycles_depth_nodes_and_edges() {
        let mut jsonlines = fs::read_to_string(canonical_fixture_path("sharing-cycle.jsonl"))
            .expect("read recursive graph fixture");
        jsonlines.push_str(
            "{\"record\":\"file\",\"id\":\"src/cycle.ts\",\"origin\":\"project\",\"selected\":true,\"diagnosticCount\":0}\n",
        );
        jsonlines.push_str(
            "{\"record\":\"fact\",\"file\":\"src/cycle.ts\",\"span\":{\"start\":0,\"end\":4},\"syntaxKind\":\"KindIdentifier\",\"actualType\":\"type:1\",\"typeAtLocation\":\"type:1\",\"typeViewStates\":{\"actual\":\"available\",\"contextual\":\"inapplicable\",\"widened\":\"same-as-actual\",\"apparent\":\"same-as-actual\",\"declared\":\"same-as-actual\"},\"complete\":true,\"recovered\":false,\"truncated\":false}\n",
        );
        let snapshot = SemanticSnapshot::from_json_lines(Cursor::new(jsonlines))
            .expect("decode recursive graph fixture");
        let facts = &snapshot.facts()[0];

        let complete = crate::inspector::GraphInspector::new(
            snapshot.graph(),
            InspectorLimits {
                max_depth: 32,
                max_nodes: 32,
                max_edges: 64,
            },
        )
        .inspect(facts);
        assert!(!complete.summary.truncated);
        assert!(complete.summary.nodes < 10, "cycle expanded repeatedly");

        for (limits, expected) in [
            (
                InspectorLimits {
                    max_depth: 0,
                    max_nodes: 32,
                    max_edges: 64,
                },
                InspectionDiagnosticCode::MaxDepth,
            ),
            (
                InspectorLimits {
                    max_depth: 32,
                    max_nodes: 1,
                    max_edges: 64,
                },
                InspectionDiagnosticCode::MaxNodes,
            ),
            (
                InspectorLimits {
                    max_depth: 32,
                    max_nodes: 32,
                    max_edges: 1,
                },
                InspectionDiagnosticCode::MaxEdges,
            ),
        ] {
            let report =
                crate::inspector::GraphInspector::new(snapshot.graph(), limits).inspect(facts);
            assert!(report.summary.truncated);
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == expected),
                "missing {expected:?}: {report:?}"
            );
        }
    }

    #[test]
    fn attachment_preserves_repeated_facts_and_unmapped_diagnostics() {
        let snapshot = Arc::new(
            SemanticSnapshot::from_json_lines(Cursor::new(minimal_snapshot(&[
                "KindIdentifier",
                "KindIdentifier",
                "KindUnprojected",
            ])))
            .expect("decode repeated and unmapped facts"),
        );
        let allocator = Allocator::default();
        let mut consumer = OxcConsumer::parse(&allocator, "src/repeated.ts", "value;\n")
            .expect("parse repeated fact source");
        let report = consumer.attach(snapshot).expect("attach repeated facts");
        assert_eq!(report.summary.mapped, 2);
        assert_eq!(report.summary.unmapped, 1);
        assert_eq!(
            report.diagnostics[0].code,
            DiagnosticCode::Unmapped,
            "unmapped fact must remain visible"
        );
        assert_eq!(consumer.mapping_report(), Some(&report));

        let node = consumer.node_for_fact(0).expect("mapped repeated node");
        assert_eq!(consumer.node_for_fact(1), Some(node));
        let attached = consumer.type_facts_for_node(node).collect::<Vec<_>>();
        assert_eq!(
            attached
                .iter()
                .map(|facts| facts.fact_index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert!(std::ptr::eq(attached[0].graph(), attached[1].graph()));
    }

    #[test]
    fn project_snapshot_attachment_preserves_response_global_fact_indices() {
        let snapshot = Arc::new(
            SemanticSnapshot::from_json_lines(Cursor::new(
                "{\"record\":\"header\",\"schemaVersion\":1,\"offsetEncoding\":\"utf8-bytes\",\"capabilities\":[]}\n\
                 {\"record\":\"file\",\"id\":\"src/a.ts\"}\n\
                 {\"record\":\"file\",\"id\":\"src/b.ts\"}\n\
                 {\"record\":\"type\",\"id\":\"type:1\",\"typeKind\":\"string\",\"display\":\"string\",\"flags\":[],\"state\":\"complete\",\"complete\":true,\"truncated\":false}\n\
                 {\"record\":\"fact\",\"file\":\"src/a.ts\",\"span\":{\"start\":0,\"end\":5},\"syntaxKind\":\"KindIdentifier\",\"actualType\":\"type:1\",\"typeAtLocation\":\"type:1\",\"typeViewStates\":{\"actual\":\"available\",\"contextual\":\"inapplicable\",\"widened\":\"same-as-actual\",\"apparent\":\"same-as-actual\",\"declared\":\"same-as-actual\"},\"complete\":true,\"recovered\":false,\"truncated\":false}\n\
                 {\"record\":\"fact\",\"file\":\"src/b.ts\",\"span\":{\"start\":0,\"end\":5},\"syntaxKind\":\"KindIdentifier\",\"actualType\":\"type:1\",\"typeAtLocation\":\"type:1\",\"typeViewStates\":{\"actual\":\"available\",\"contextual\":\"inapplicable\",\"widened\":\"same-as-actual\",\"apparent\":\"same-as-actual\",\"declared\":\"same-as-actual\"},\"complete\":true,\"recovered\":false,\"truncated\":false}\n",
            ))
            .expect("decode multi-file snapshot"),
        );

        for (file, expected_fact_index) in [("src/a.ts", 0), ("src/b.ts", 1)] {
            let allocator = Allocator::default();
            let mut consumer =
                OxcConsumer::parse(&allocator, file, "value;\n").expect("parse multi-file source");
            let report = consumer
                .attach_file(Arc::clone(&snapshot))
                .expect("attach one project file");
            assert_eq!(report.mappings[0].fact_index, expected_fact_index);
            assert!(consumer.node_for_fact(expected_fact_index).is_some());
        }
    }

    fn canonical_fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("internal/tsfacts/testdata/canonical/v0")
            .join(name)
    }

    fn load_canonical_snapshot(name: &str) -> Arc<SemanticSnapshot> {
        let path = canonical_fixture_path(name);
        let file = fs::File::open(&path)
            .unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
        Arc::new(
            SemanticSnapshot::from_json_lines(BufReader::new(file))
                .unwrap_or_else(|error| panic!("decode {}: {error}", path.display())),
        )
    }

    fn candidate_type<'a>(
        candidate: &'a crate::candidate::PrimitiveLiteralCandidate,
        id: &str,
    ) -> &'a crate::candidate::CandidateTypeRecord {
        candidate
            .types
            .iter()
            .find(|record| record.id.as_str() == id)
            .unwrap_or_else(|| panic!("candidate omitted {id}"))
    }

    fn minimal_snapshot(syntax_kinds: &[&str]) -> String {
        let mut output = String::from(
            "{\"record\":\"header\",\"schemaVersion\":1,\"offsetEncoding\":\"utf8-bytes\",\"capabilities\":[]}\n\
             {\"record\":\"file\",\"id\":\"src/repeated.ts\"}\n\
             {\"record\":\"type\",\"id\":\"type:1\",\"typeKind\":\"string\",\"display\":\"string\",\"flags\":[],\"state\":\"complete\",\"complete\":true,\"truncated\":false}\n",
        );
        for syntax_kind in syntax_kinds {
            output.push_str(&format!(
                "{{\"record\":\"fact\",\"file\":\"src/repeated.ts\",\"span\":{{\"start\":0,\"end\":5}},\"syntaxKind\":\"{syntax_kind}\",\"actualType\":\"type:1\",\"typeAtLocation\":\"type:1\",\"typeViewStates\":{{\"actual\":\"available\",\"contextual\":\"inapplicable\",\"widened\":\"same-as-actual\",\"apparent\":\"same-as-actual\",\"declared\":\"same-as-actual\"}},\"complete\":true,\"recovered\":false,\"truncated\":false}}\n"
            ));
        }
        output
    }
}

use std::collections::{BTreeSet, VecDeque};

use serde::Serialize;

use crate::candidate::PrimitiveLiteralCandidate;
use crate::facts::{
    EntityState, GraphIssue, GraphRef, OccurrenceTypeFacts, TypeGraph, TypeView, TypeViewState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectorLimits {
    pub max_depth: u32,
    pub max_nodes: usize,
    pub max_edges: usize,
}

impl Default for InspectorLimits {
    fn default() -> Self {
        Self {
            max_depth: 32,
            max_nodes: 4096,
            max_edges: 16384,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionReport {
    pub fact: InspectedFactStatus,
    pub roots: Vec<InspectedRoot>,
    pub primitive_literal_candidate: PrimitiveLiteralCandidate,
    pub nodes: Vec<InspectedNode>,
    pub edges: Vec<InspectedEdge>,
    pub diagnostics: Vec<InspectionDiagnostic>,
    pub summary: InspectionSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectedFactStatus {
    pub complete: bool,
    pub recovered: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectedRoot {
    pub view: TypeView,
    pub state: TypeViewState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectedNode {
    pub reference: GraphRef,
    pub category: String,
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<EntityState>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<GraphIssue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectedEdge {
    pub from: GraphRef,
    pub label: String,
    pub to: GraphRef,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InspectionDiagnosticCode {
    RootInapplicable,
    RootUnavailable,
    EntityTruncated,
    EntityUnsupported,
    EntityError,
    MissingNode,
    MaxDepth,
    MaxNodes,
    MaxEdges,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionDiagnostic {
    pub code: InspectionDiagnosticCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<TypeView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<GraphRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<TypeViewState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionSummary {
    pub nodes: usize,
    pub edges: usize,
    pub truncated: bool,
}

pub struct GraphInspector<'a> {
    graph: &'a TypeGraph,
    limits: InspectorLimits,
}

impl<'a> GraphInspector<'a> {
    pub fn new(graph: &'a TypeGraph, limits: InspectorLimits) -> Self {
        Self { graph, limits }
    }

    pub fn inspect(&self, facts: &OccurrenceTypeFacts) -> InspectionReport {
        let mut diagnostics = BTreeSet::new();
        let mut queue = VecDeque::new();
        let roots = facts
            .roots()
            .into_iter()
            .map(|root| {
                let type_id = root.type_id.map(|id| id.as_str().to_owned());
                if let Some(id) = root.type_id {
                    queue.push_back((GraphRef::Type(id.clone()), 0));
                } else {
                    diagnostics.insert(InspectionDiagnostic {
                        code: match root.state {
                            TypeViewState::Inapplicable => {
                                InspectionDiagnosticCode::RootInapplicable
                            }
                            TypeViewState::Unavailable => InspectionDiagnosticCode::RootUnavailable,
                            TypeViewState::Available | TypeViewState::SameAsActual => {
                                InspectionDiagnosticCode::MissingNode
                            }
                        },
                        view: Some(root.view),
                        node: None,
                        limit: None,
                        state: Some(root.state),
                    });
                }
                InspectedRoot {
                    view: root.view,
                    state: root.state,
                    type_id,
                }
            })
            .collect();

        let mut visited = BTreeSet::new();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut edge_budget_exhausted = false;

        while let Some((reference, depth)) = queue.pop_front() {
            if visited.contains(&reference) {
                continue;
            }
            if nodes.len() >= self.limits.max_nodes {
                diagnostics.insert(InspectionDiagnostic {
                    code: InspectionDiagnosticCode::MaxNodes,
                    view: None,
                    node: Some(reference),
                    limit: Some(self.limits.max_nodes),
                    state: None,
                });
                continue;
            }
            let Some(node) = self.inspect_node(&reference) else {
                diagnostics.insert(InspectionDiagnostic {
                    code: InspectionDiagnosticCode::MissingNode,
                    view: None,
                    node: Some(reference),
                    limit: None,
                    state: None,
                });
                continue;
            };
            visited.insert(reference.clone());
            if let Some(state) = node.state {
                let code = match state {
                    EntityState::Complete => None,
                    EntityState::Truncated => Some(InspectionDiagnosticCode::EntityTruncated),
                    EntityState::Unsupported => Some(InspectionDiagnosticCode::EntityUnsupported),
                    EntityState::Error => Some(InspectionDiagnosticCode::EntityError),
                };
                if let Some(code) = code {
                    diagnostics.insert(InspectionDiagnostic {
                        code,
                        view: None,
                        node: Some(reference.clone()),
                        limit: None,
                        state: None,
                    });
                }
            }
            nodes.push(node);

            let outgoing = self
                .graph
                .edges(&reference)
                .expect("inspected graph node has edges");
            if depth >= self.limits.max_depth {
                if !outgoing.is_empty() {
                    diagnostics.insert(InspectionDiagnostic {
                        code: InspectionDiagnosticCode::MaxDepth,
                        view: None,
                        node: Some(reference),
                        limit: Some(self.limits.max_depth as usize),
                        state: None,
                    });
                }
                continue;
            }

            for edge in outgoing {
                if edge_budget_exhausted || edges.len() >= self.limits.max_edges {
                    edge_budget_exhausted = true;
                    diagnostics.insert(InspectionDiagnostic {
                        code: InspectionDiagnosticCode::MaxEdges,
                        view: None,
                        node: Some(reference.clone()),
                        limit: Some(self.limits.max_edges),
                        state: None,
                    });
                    break;
                }
                queue.push_back((edge.target.clone(), depth + 1));
                edges.push(InspectedEdge {
                    from: reference.clone(),
                    label: edge.label,
                    to: edge.target,
                });
            }
        }

        let diagnostics = diagnostics.into_iter().collect::<Vec<_>>();
        InspectionReport {
            fact: InspectedFactStatus {
                complete: facts.complete,
                recovered: facts.recovered,
                truncated: facts.truncated,
            },
            roots,
            primitive_literal_candidate: PrimitiveLiteralCandidate::build(self.graph, facts),
            summary: InspectionSummary {
                nodes: nodes.len(),
                edges: edges.len(),
                truncated: diagnostics.iter().any(|diagnostic| {
                    matches!(
                        diagnostic.code,
                        InspectionDiagnosticCode::MaxDepth
                            | InspectionDiagnosticCode::MaxNodes
                            | InspectionDiagnosticCode::MaxEdges
                    )
                }),
            },
            nodes,
            edges,
            diagnostics,
        }
    }

    fn inspect_node(&self, reference: &GraphRef) -> Option<InspectedNode> {
        match reference {
            GraphRef::Type(id) => self.graph.type_record(id).map(|record| InspectedNode {
                reference: reference.clone(),
                category: format!("type:{}", wire_name(record.type_kind)),
                display: record.display.clone(),
                state: Some(record.state),
                issues: record.issues.clone(),
                complete: Some(record.complete),
                truncated: Some(record.truncated),
            }),
            GraphRef::Symbol(id) => self.graph.symbol(id).map(|record| InspectedNode {
                reference: reference.clone(),
                category: "symbol".to_owned(),
                display: record.name.clone(),
                state: Some(record.state),
                issues: record.issues.clone(),
                complete: Some(record.complete),
                truncated: Some(record.truncated),
            }),
            GraphRef::Signature(id) => self.graph.signature(id).map(|record| InspectedNode {
                reference: reference.clone(),
                category: format!("signature:{}", wire_name(record.signature_kind)),
                display: record.id.as_str().to_owned(),
                state: Some(record.state),
                issues: record.issues.clone(),
                complete: Some(record.complete),
                truncated: Some(record.truncated),
            }),
            GraphRef::Declaration(id) => self.graph.declaration(id).map(|record| InspectedNode {
                reference: reference.clone(),
                category: "declaration".to_owned(),
                display: format!(
                    "{}:{}-{} {}",
                    record.file, record.span.start, record.span.end, record.syntax_kind
                ),
                state: None,
                issues: Vec::new(),
                complete: None,
                truncated: None,
            }),
        }
    }
}

fn wire_name(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .expect("serialize closed protocol enum")
        .as_str()
        .expect("protocol enum serializes as a string")
        .to_owned()
}

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

pub const CONTRACT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub file: String,
    pub span: Span,
    pub syntax_kind: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NormalizationRule {
    KindAlias,
    ProtocolInnerSpan,
    ProtocolOuterSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Normalization {
    pub span: Span,
    pub syntax_kind: String,
    pub rule: NormalizationRule,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeCandidate {
    pub node_id: u32,
    pub file: String,
    pub span: Span,
    pub syntax_kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub normalizations: Vec<Normalization>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    pub fact_index: usize,
    pub node_id: u32,
    #[serde(rename = "match")]
    pub match_kind: MatchKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<NormalizationRule>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchKind {
    Exact,
    Normalized,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub node_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<NormalizationRule>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub fact_index: usize,
    pub file: String,
    pub span: Span,
    pub syntax_kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticCode {
    Unmapped,
    MultiplyMapped,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub facts: u32,
    pub mapped: u32,
    pub exact: u32,
    pub normalized: u32,
    pub unmapped: u32,
    pub multiply_mapped: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindCoverage {
    pub syntax_kind: String,
    pub metrics: Metrics,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub contract_version: u32,
    pub mappings: Vec<Mapping>,
    pub diagnostics: Vec<Diagnostic>,
    pub summary: Metrics,
    pub by_syntax_kind: Vec<KindCoverage>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Key {
    file: String,
    span: Span,
    syntax_kind: String,
}

impl Key {
    fn new(file: &str, span: Span, syntax_kind: &str) -> Self {
        Self {
            file: file.to_owned(),
            span,
            syntax_kind: syntax_kind.to_owned(),
        }
    }
}

pub fn correlate(facts: &[Occurrence], nodes: &[NodeCandidate]) -> Result<Report, String> {
    let mut exact: HashMap<Key, BTreeSet<u32>> = HashMap::new();
    let mut normalized: HashMap<Key, BTreeMap<u32, NormalizationRule>> = HashMap::new();
    let mut identities = BTreeSet::new();

    for (index, node) in nodes.iter().enumerate() {
        validate_identity(&node.file, node.span, &node.syntax_kind)
            .map_err(|error| format!("nodes[{index}]: {error}"))?;
        if !identities.insert((node.file.clone(), node.node_id)) {
            return Err(format!(
                "nodes[{index}].nodeId duplicates nodeId {} in file {:?}",
                node.node_id, node.file
            ));
        }
        exact
            .entry(Key::new(&node.file, node.span, &node.syntax_kind))
            .or_default()
            .insert(node.node_id);

        for (normalization_index, normalization) in node.normalizations.iter().enumerate() {
            validate_normalization(node, normalization).map_err(|error| {
                format!("nodes[{index}].normalizations[{normalization_index}]: {error}")
            })?;
            normalized
                .entry(Key::new(
                    &node.file,
                    normalization.span,
                    &normalization.syntax_kind,
                ))
                .or_default()
                .entry(node.node_id)
                .and_modify(|rule| *rule = (*rule).min(normalization.rule))
                .or_insert(normalization.rule);
        }
    }

    let mut report = Report {
        contract_version: CONTRACT_VERSION,
        mappings: Vec::with_capacity(facts.len()),
        diagnostics: Vec::new(),
        summary: Metrics::default(),
        by_syntax_kind: Vec::new(),
    };
    let mut coverage: BTreeMap<String, Metrics> = BTreeMap::new();

    for (fact_index, fact) in facts.iter().enumerate() {
        validate_identity(&fact.file, fact.span, &fact.syntax_kind)
            .map_err(|error| format!("facts[{fact_index}]: {error}"))?;
        let key = Key::new(&fact.file, fact.span, &fact.syntax_kind);
        let exact_candidates = exact.get(&key).map_or_else(Vec::new, |ids| {
            ids.iter()
                .map(|node_id| Candidate {
                    node_id: *node_id,
                    rule: None,
                })
                .collect()
        });
        let normalized_candidates = normalized.get(&key).map_or_else(Vec::new, |ids| {
            ids.iter()
                .map(|(node_id, rule)| Candidate {
                    node_id: *node_id,
                    rule: Some(*rule),
                })
                .collect()
        });

        let metrics = coverage.entry(fact.syntax_kind.clone()).or_default();
        metrics.facts += 1;
        report.summary.facts += 1;

        let candidates = if exact_candidates.is_empty() {
            normalized_candidates
        } else {
            exact_candidates
        };
        if candidates.len() == 1 {
            let candidate = &candidates[0];
            let match_kind = if candidate.rule.is_some() {
                metrics.normalized += 1;
                report.summary.normalized += 1;
                MatchKind::Normalized
            } else {
                metrics.exact += 1;
                report.summary.exact += 1;
                MatchKind::Exact
            };
            metrics.mapped += 1;
            report.summary.mapped += 1;
            report.mappings.push(Mapping {
                fact_index,
                node_id: candidate.node_id,
                match_kind,
                rule: candidate.rule,
            });
        } else {
            let code = if candidates.is_empty() {
                metrics.unmapped += 1;
                report.summary.unmapped += 1;
                DiagnosticCode::Unmapped
            } else {
                metrics.multiply_mapped += 1;
                report.summary.multiply_mapped += 1;
                DiagnosticCode::MultiplyMapped
            };
            report.diagnostics.push(Diagnostic {
                code,
                fact_index,
                file: fact.file.clone(),
                span: fact.span,
                syntax_kind: fact.syntax_kind.clone(),
                candidates,
            });
        }
    }

    report.by_syntax_kind = coverage
        .into_iter()
        .map(|(syntax_kind, metrics)| KindCoverage {
            syntax_kind,
            metrics,
        })
        .collect();
    Ok(report)
}

fn validate_identity(file: &str, span: Span, syntax_kind: &str) -> Result<(), String> {
    if file.is_empty() {
        return Err("file is required".to_owned());
    }
    if span.end < span.start {
        return Err(format!("invalid span [{}, {})", span.start, span.end));
    }
    if syntax_kind.is_empty() {
        return Err("syntaxKind is required".to_owned());
    }
    Ok(())
}

fn validate_normalization(
    node: &NodeCandidate,
    normalization: &Normalization,
) -> Result<(), String> {
    validate_identity(&node.file, normalization.span, &normalization.syntax_kind)?;
    let same_span = node.span == normalization.span;
    match normalization.rule {
        NormalizationRule::KindAlias if !same_span => {
            Err("kind-alias requires the canonical span".to_owned())
        }
        NormalizationRule::ProtocolInnerSpan
            if same_span
                || normalization.span.start < node.span.start
                || normalization.span.end > node.span.end =>
        {
            Err("protocol-inner-span requires a proper subspan".to_owned())
        }
        NormalizationRule::ProtocolOuterSpan
            if same_span
                || normalization.span.start > node.span.start
                || normalization.span.end < node.span.end =>
        {
            Err("protocol-outer-span requires a proper enclosing span".to_owned())
        }
        _ => Ok(()),
    }
}

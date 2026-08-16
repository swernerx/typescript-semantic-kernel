use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::contract::{NodeCandidate, Occurrence};

#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub description: String,
    pub sources: BTreeMap<String, String>,
    pub facts: Vec<Occurrence>,
    pub nodes: Vec<NodeCandidate>,
    pub expected: serde_json::Value,
}

pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("internal/occurrencemap/testdata/v1")
}

pub fn load_fixtures() -> Result<Vec<(PathBuf, Fixture)>, String> {
    let directory = fixture_dir();
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read {}: {error}", directory.display()))?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "json")
    });
    paths.sort();

    paths
        .into_iter()
        .map(|path| {
            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            let fixture = serde_json::from_str(&contents)
                .map_err(|error| format!("decode {}: {error}", path.display()))?;
            Ok((path, fixture))
        })
        .collect()
}

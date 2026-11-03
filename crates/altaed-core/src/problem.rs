//! A local problem instance belongs to one document, never to its URL.
use crate::document::TestCase;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemId(Uuid);
impl Default for ProblemId {
    fn default() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProblemInput {
    Stdin,
    File { file_name: String },
    Regex { pattern: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProblemOutput {
    Stdout,
    File { file_name: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportBatch {
    pub id: String,
    pub size: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub id: ProblemId,
    pub title: String,
    pub group: String,
    pub url: String,
    pub time_limit_ms: Option<u64>,
    pub memory_limit_mb: Option<u64>,
    pub interactive: bool,
    pub input: ProblemInput,
    pub output: ProblemOutput,
    pub test_type: String,
    /// Original imported samples; editable cases are separate document values.
    pub samples: Vec<TestCase>,
    pub languages: BTreeMap<String, serde_json::Value>,
    pub batch: Option<ImportBatch>,
    #[serde(default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}
impl Problem {
    pub fn independent_copy(&self) -> Self {
        Self {
            id: ProblemId::default(),
            ..self.clone()
        }
    }
    pub fn supports_sample_runner(&self) -> bool {
        !self.interactive
            && self.input == ProblemInput::Stdin
            && self.output == ProblemOutput::Stdout
    }
}

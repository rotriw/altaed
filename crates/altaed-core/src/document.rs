use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(Uuid);

impl std::fmt::Display for DocumentId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TestCase {
    pub input: String,
    pub expected_output: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentSettings {
    pub compiler_path: Option<PathBuf>,
    pub compiler_arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
    #[serde(default)]
    pub execution: crate::runner::ExecutionSettings,
    #[serde(default)]
    pub extensions: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Default for DocumentSettings {
    fn default() -> Self {
        Self {
            compiler_path: None,
            compiler_arguments: vec!["-std=c++20".into(), "-O2".into()],
            working_directory: None,
            execution: Default::default(),
            extensions: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub problem_url: Option<String>,
    pub problem_title: Option<String>,
    pub tests: Vec<TestCase>,
    pub settings: DocumentSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeDocument {
    pub id: DocumentId,
    pub path: Option<PathBuf>,
    // Display numbering survives reordering; DocumentId remains the identity.
    #[serde(default)]
    untitled_number: Option<u64>,
    pub text: String,
    pub metadata: DocumentMetadata,
    saved_text: String,
    saved_metadata: DocumentMetadata,
}

impl CodeDocument {
    pub fn untitled() -> Self {
        Self::untitled_with_settings(DocumentSettings::default())
    }
    pub fn untitled_with_settings(settings: DocumentSettings) -> Self {
        let metadata = DocumentMetadata {
            settings,
            ..Default::default()
        };
        Self {
            id: DocumentId(Uuid::new_v4()),
            path: None,
            untitled_number: None,
            text: String::new(),
            metadata: metadata.clone(),
            saved_text: String::new(),
            saved_metadata: metadata,
        }
    }
    pub fn is_dirty(&self) -> bool {
        self.text != self.saved_text || self.metadata != self.saved_metadata
    }
    pub fn mark_saved(&mut self) {
        self.saved_text.clone_from(&self.text);
        self.saved_metadata.clone_from(&self.metadata);
    }
    pub fn fork(&self, path: PathBuf) -> Self {
        let mut copy = self.clone();
        copy.id = DocumentId(Uuid::new_v4());
        copy.path = Some(path);
        copy
    }
    pub fn untitled_number(&self) -> Option<u64> {
        self.untitled_number
    }
    pub fn ensure_untitled_number(
        &mut self,
        used: &std::collections::BTreeSet<u64>,
        preferred: u64,
    ) {
        if self.path.is_none() && self.untitled_number.is_none() {
            // A finite set of N numbers always leaves a free value in 1..=N+1.
            self.untitled_number = if !used.contains(&preferred) {
                Some(preferred)
            } else {
                (1..=used.len() as u64 + 1).find(|number| !used.contains(number))
            };
        }
    }
    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                self.untitled_number
                    .map(|number| format!("未命名 {number}"))
                    .unwrap_or_else(|| "未命名".into())
            })
    }
    pub fn title(&self) -> String {
        format!(
            "{}{}",
            self.display_name(),
            if self.is_dirty() { " *" } else { "" }
        )
    }
}

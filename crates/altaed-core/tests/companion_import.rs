use altaed_core::{
    companion,
    document::{CodeDocument, DocumentSettings},
    session::Session,
    storage,
};
use anyhow::Result;
use std::{fs, path::PathBuf};
use uuid::Uuid;
const PAYLOAD: &[u8] = include_bytes!("fixtures/companion.json");
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("altaed-import-{}", Uuid::new_v4()));
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn imports_are_independent_and_survive_save_as_and_recovery() -> Result<()> {
    let scratch = Scratch::new()?;
    let first_id = companion::receive_problem(&scratch.0, PAYLOAD, DocumentSettings::default())?;
    let second_id = companion::receive_problem(&scratch.0, PAYLOAD, DocumentSettings::default())?;
    assert_ne!(first_id, second_id);
    let receipts = companion::pending_documents(&scratch.0)?;
    assert_eq!(receipts.len(), 2);
    let mut first = receipts[0].document.clone();
    let second = receipts[1].document.clone();
    let problem = first.metadata.problem.as_ref().unwrap();
    assert_ne!(problem.id, second.metadata.problem.as_ref().unwrap().id);
    assert_eq!(problem.time_limit_ms, Some(1000));
    assert_eq!(problem.languages["java"]["mainClass"], "Main");
    assert_eq!(problem.additional["futureField"]["preserved"], true);
    assert_eq!(first.metadata.tests[1].expected_output.as_deref(), Some(""));
    first.metadata.tests[0].expected_output = Some("local change".into());
    assert_eq!(
        first.metadata.problem.as_ref().unwrap().samples[0]
            .expected_output
            .as_deref(),
        Some("3\n")
    );
    assert_eq!(
        second.metadata.tests[0].expected_output.as_deref(),
        Some("3\n")
    );
    let mut fork = first.fork(scratch.0.join("other.cpp"));
    assert_ne!(
        fork.metadata.problem.as_ref().unwrap().id,
        first.metadata.problem.as_ref().unwrap().id
    );
    fork.metadata.tests[0].input = "independent".into();
    storage::save_document(&mut fork)?;
    let reopened = storage::open_document(&scratch.0.join("other.cpp"))?;
    assert_eq!(reopened.metadata, fork.metadata);
    storage::save_session(
        &scratch.0,
        &Session {
            documents: vec![first, second],
            active: 1,
        },
    )?;
    let restored = storage::load_session(&scratch.0)?;
    assert_eq!(restored.documents.len(), 2);
    assert!(restored.documents[0].metadata.problem.is_some());
    // A crash before acknowledgment leaves receipts; document IDs allow the UI
    // to recognize already checkpointed imports instead of opening duplicates.
    for receipt in companion::pending_documents(&scratch.0)? {
        assert!(
            restored
                .documents
                .iter()
                .any(|document| document.id == receipt.document.id)
        );
        companion::acknowledge(&scratch.0, &receipt)?;
    }
    assert!(companion::pending_documents(&scratch.0)?.is_empty());
    Ok(())
}
#[test]
fn preserves_file_and_interactive_modes_and_rejects_invalid_payloads() -> Result<()> {
    let mut value: serde_json::Value = serde_json::from_slice(PAYLOAD)?;
    value["interactive"] = true.into();
    value["input"] = serde_json::json!({"type":"file","fileName":"../../not-materialized.in"});
    let problem = companion::parse_problem(&serde_json::to_vec(&value)?)?;
    assert!(!problem.supports_sample_runner());
    let document = CodeDocument::from_problem(problem, DocumentSettings::default());
    assert!(document.is_dirty());
    value["url"] = "javascript:alert(1)".into();
    assert!(companion::parse_problem(&serde_json::to_vec(&value)?).is_err());
    assert!(companion::parse_problem(b"{}").is_err());
    Ok(())
}

#[test]
fn legacy_sessions_without_problem_data_are_read_and_upgraded_on_write() -> Result<()> {
    let scratch = Scratch::new()?;
    let session = Session {
        documents: vec![CodeDocument::untitled()],
        active: 0,
    };
    let mut value = serde_json::to_value(&session)?;
    let document = &mut value["documents"][0];
    document["metadata"]
        .as_object_mut()
        .unwrap()
        .remove("problem");
    document["saved_metadata"]
        .as_object_mut()
        .unwrap()
        .remove("problem");
    for key in ["metadata", "saved_metadata"] {
        document[key]["settings"]
            .as_object_mut()
            .unwrap()
            .remove("compiler_options");
    }
    let path = scratch.0.join("session.json");
    for version in [1, 2, 3, 4] {
        let legacy = serde_json::json!({"version":version,"value":value});
        fs::write(&path, serde_json::to_vec(&legacy)?)?;
        let restored = storage::load_session(&scratch.0)?;
        assert!(restored.documents[0].metadata.problem.is_none());
        assert_eq!(
            restored.documents[0].metadata.settings.compiler_options,
            Default::default()
        );
        storage::save_session(&scratch.0, &restored)?;
        let upgraded: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(upgraded["version"], 5);
    }
    Ok(())
}

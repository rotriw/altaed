use crate::{document::CodeDocument, session::Session};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    path::{Path, PathBuf},
};
const FORMAT_VERSION: u32 = 1;
#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    version: u32,
    value: T,
}

/// Orders asynchronous checkpoints so a late save cannot replace a newer exit snapshot.
pub struct SessionStore {
    directory: PathBuf,
    next: std::sync::atomic::AtomicU64,
    committed: std::sync::Mutex<u64>,
}
impl SessionStore {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            next: std::sync::atomic::AtomicU64::new(1),
            committed: std::sync::Mutex::new(0),
        }
    }
    pub fn reserve(&self) -> u64 {
        self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
    pub fn save(&self, revision: u64, session: &Session) -> Result<()> {
        let mut committed = self
            .committed
            .lock()
            .map_err(|_| anyhow::anyhow!("session writer unavailable"))?;
        if revision <= *committed {
            return Ok(());
        }
        save_session(&self.directory, session)?;
        *committed = revision;
        Ok(())
    }
}

pub fn data_directory() -> Result<PathBuf> {
    crate::platform::data_directory()
}
pub(crate) fn write_value<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    crate::atomic_file::replace(
        path,
        &serde_json::to_vec_pretty(&Envelope {
            version: FORMAT_VERSION,
            value,
        })?,
    )
}
pub(crate) fn read_value<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let record: Envelope<T> = serde_json::from_slice(&fs::read(path)?)?;
    if record.version != FORMAT_VERSION {
        bail!("unsupported storage version {}", record.version);
    }
    Ok(record.value)
}
fn metadata_path(path: &Path) -> Result<PathBuf> {
    let name = path.file_name().context("source has no filename")?;
    let mut filename = name.to_os_string();
    filename.push(".json");
    Ok(path
        .parent()
        .context("source has no parent")?
        .join(".altaed")
        .join(filename))
}
pub fn open_document(path: &Path) -> Result<CodeDocument> {
    let path = fs::canonicalize(path)?;
    let metadata = metadata_path(&path)?;
    let pending = metadata.with_extension("pending");
    if pending.exists() {
        let mut document: CodeDocument = read_value(&pending)?;
        document.path = Some(path);
        return Ok(document);
    }
    let mut document = if metadata.exists() {
        read_value(&metadata)?
    } else {
        CodeDocument::untitled()
    };
    document.path = Some(path.clone());
    document.text =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    document.mark_saved();
    Ok(document)
}
pub fn save_document(document: &mut CodeDocument) -> Result<()> {
    let path = document
        .path
        .as_ref()
        .context("document needs a save path")?;
    // The pending record preserves both parts across an interrupted two-file save.
    let metadata = metadata_path(path)?;
    let pending = metadata.with_extension("pending");
    write_value(&pending, document)?;
    crate::atomic_file::replace(path, document.text.as_bytes())?;
    let mut saved = document.clone();
    saved.mark_saved();
    write_value(&metadata, &saved)?;
    fs::remove_file(pending)?;
    *document = saved;
    Ok(())
}
pub fn save_session(directory: &Path, session: &Session) -> Result<()> {
    write_value(&directory.join("session.json"), session)
}
pub fn load_session(directory: &Path) -> Result<Session> {
    let path = directory.join("session.json");
    if path.exists() {
        read_value(&path)
    } else {
        Ok(Session::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("altaed-test-{}", Uuid::new_v4())))
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn stale_checkpoint_cannot_replace_newer_session() -> Result<()> {
        let root = Temp::new();
        let store = SessionStore::new(root.0.clone());
        let old = store.reserve();
        let new = store.reserve();
        let mut latest = Session::default();
        latest.ensure_document();
        latest.documents[0].text = "latest".into();
        store.save(new, &latest)?;
        store.save(old, &Session::default())?;
        assert_eq!(load_session(&root.0)?.documents[0].text, "latest");
        Ok(())
    }
    #[test]
    fn save_as_is_independent_and_directory_is_portable() -> Result<()> {
        let root = Temp::new();
        let moved = Temp::new();
        let mut first = CodeDocument::untitled();
        first.path = Some(root.0.join("自由.cpp"));
        first.text = "你好".into();
        first.metadata.tests.push(Default::default());
        save_document(&mut first)?;
        let mut second = first.fork(root.0.join("other.cpp"));
        second.metadata.tests[0].input = "counterexample".into();
        save_document(&mut second)?;
        assert_ne!(first.id, second.id);
        assert!(first.metadata.tests[0].input.is_empty());
        fs::rename(&root.0, &moved.0)?;
        let reopened = open_document(&moved.0.join("自由.cpp"))?;
        assert_eq!(reopened.id, first.id);
        assert_eq!(reopened.text, "你好");
        assert!(!reopened.is_dirty());
        Ok(())
    }
    #[test]
    fn recovery_preserves_unsaved_metadata_and_discard() -> Result<()> {
        let root = Temp::new();
        let mut doc = CodeDocument::untitled();
        doc.metadata.problem_title = Some("draft".into());
        assert!(doc.is_dirty());
        let mut session = Session {
            documents: vec![doc],
            active: 0,
        };
        save_session(&root.0, &session)?;
        assert!(load_session(&root.0)?.documents[0].is_dirty());
        session.documents.clear();
        save_session(&root.0, &session)?;
        assert!(load_session(&root.0)?.documents.is_empty());
        Ok(())
    }
    #[test]
    fn failed_save_keeps_dirty_content_and_recovery_record() -> Result<()> {
        let root = Temp::new();
        fs::create_dir_all(root.0.join("blocked"))?;
        let mut doc = CodeDocument::untitled();
        doc.path = Some(root.0.join("blocked"));
        doc.text = "unsaved".into();
        assert!(save_document(&mut doc).is_err());
        assert!(doc.is_dirty());
        assert!(
            metadata_path(doc.path.as_ref().unwrap())?
                .with_extension("pending")
                .exists()
        );
        Ok(())
    }
}

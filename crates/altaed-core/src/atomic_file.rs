//! Single-file writes shared by document and application storage. Staging and
//! cleanup are common; each caller chooses replace-existing or create-only.
//! These operations do not make a source/sidecar pair into one transaction.
use anyhow::{Context, Result};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub fn replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let staged = stage(path, bytes)?;
    fs::rename(&staged.path, path).with_context(|| format!("cannot save {}", path.display()))
}

/// A hard-link commit is atomic and fails if another process created the target.
/// Unlike rename, it cannot replace a user's concurrently created configuration.
pub fn create(path: &Path, bytes: &[u8]) -> Result<bool> {
    let staged = stage(path, bytes)?;
    match fs::hard_link(&staged.path, path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error).with_context(|| format!("cannot create {}", path.display())),
    }
}

fn stage(destination: &Path, bytes: &[u8]) -> Result<StagedFile> {
    let parent = destination.parent().context("path has no parent")?;
    fs::create_dir_all(parent)?;
    let staged = StagedFile {
        path: parent.join(format!(".altaed-{}.tmp", Uuid::new_v4())),
    };
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staged.path)?;
    match fs::metadata(destination) {
        Ok(metadata) => file.set_permissions(metadata.permissions())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot inspect {}", destination.display()));
        }
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(staged)
}

// The guard also cleans partial writes and failed commits without hiding their
// original error. After rename, NotFound simply means the commit consumed it.
struct StagedFile {
    path: PathBuf,
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "cannot remove staging file {}: {error}",
                self.path.display()
            );
        }
    }
}

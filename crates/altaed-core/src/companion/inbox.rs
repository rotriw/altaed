//! Receipt files survive crashes until a session containing the import is saved.
use crate::document::CodeDocument;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_PENDING_IMPORTS: usize = 128;
const MAX_RECORD_BYTES: u64 = (super::server::MAX_BODY_BYTES as u64) * 3 + 4096;
const FORMAT_VERSION: u32 = 4;
#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    document: CodeDocument,
}
#[derive(Clone)]
pub struct PendingImport {
    pub document: CodeDocument,
    file_name: String,
}
fn directory(root: &Path) -> PathBuf {
    root.join("companion-inbox")
}
pub(super) fn store(root: &Path, document: CodeDocument) -> Result<()> {
    let directory = directory(root);
    fs::create_dir_all(&directory).context("无法创建题目导入收件目录")?;
    ensure!(
        fs::read_dir(&directory)?.count() < MAX_PENDING_IMPORTS,
        "待接收题目过多，请先在 IDE 中处理"
    );
    let path = directory.join(format!("{}.json", document.id));
    let bytes = serde_json::to_vec(&Record {
        version: FORMAT_VERSION,
        document,
    })?;
    ensure!(bytes.len() as u64 <= MAX_RECORD_BYTES, "导入记录过大");
    ensure!(crate::atomic_file::create(&path, &bytes)?, "导入标识已存在");
    Ok(())
}
pub fn pending_documents(root: &Path) -> Result<Vec<PendingImport>> {
    let directory = directory(root);
    if !directory.try_exists()? {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|value| value == "json")
        {
            files.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    files.sort();
    files
        .into_iter()
        .take(16)
        .map(|(_, path)| {
            let mut bytes = Vec::new();
            fs::File::open(&path)?
                .take(MAX_RECORD_BYTES + 1)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 <= MAX_RECORD_BYTES,
                "导入记录过大：{}",
                path.display()
            );
            let record: Record = serde_json::from_slice(&bytes)
                .with_context(|| format!("无法读取导入记录 {}", path.display()))?;
            ensure!(
                (1..=FORMAT_VERSION).contains(&record.version),
                "导入记录版本不受支持"
            );
            let file_name = format!("{}.json", record.document.id);
            ensure!(
                path.file_name()
                    .is_some_and(|name| name == file_name.as_str()),
                "导入记录身份不一致"
            );
            Ok(PendingImport {
                document: record.document,
                file_name,
            })
        })
        .collect()
}
pub fn acknowledge(root: &Path, import: &PendingImport) -> Result<()> {
    match fs::remove_file(directory(root).join(&import.file_name)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("题目已恢复，但无法清理导入收据"),
    }
}

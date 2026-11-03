//! Bound test files are read on a worker before execution. The original document
//! retains links and inline fallbacks; resolved strings belong to one run only.
use crate::document::CodeDocument;
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

pub const PREVIEW_BYTES: usize = 16 * 1024;
pub const MAX_EDITOR_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct TextFile {
    pub text: String,
    modified: SystemTime,
}
pub fn resolve_path(document: &CodeDocument, path: &Path) -> Result<PathBuf> {
    document.resolve_attachment_path(path)
}
pub fn stored_path(document: &CodeDocument, absolute: &Path) -> PathBuf {
    document.stored_attachment_path(absolute)
}
pub fn read_text(path: &Path, limit: u64) -> Result<TextFile> {
    let mut file =
        fs::File::open(path).with_context(|| format!("无法打开样例文件 {}", path.display()))?;
    let before = file.metadata()?;
    ensure!(before.is_file(), "样例路径不是普通文件：{}", path.display());
    ensure!(
        before.len() <= limit,
        "样例文件超过允许的读取上限（{limit} 字节）：{}",
        path.display()
    );
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "读取时样例文件超过上限：{}",
        path.display()
    );
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len() && before.modified()? == after.modified()?,
        "样例文件在读取时发生变化，请重试：{}",
        path.display()
    );
    let text = String::from_utf8(bytes)
        .with_context(|| format!("样例文件必须是 UTF-8 文本：{}", path.display()))?;
    Ok(TextFile {
        text,
        modified: after.modified()?,
    })
}
pub fn preview(path: &Path) -> Result<String> {
    let file = fs::File::open(path).with_context(|| format!("无法打开 {}", path.display()))?;
    ensure!(file.metadata()?.is_file(), "样例路径不是普通文件");
    let mut bytes = Vec::new();
    file.take(PREVIEW_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() > PREVIEW_BYTES;
    bytes.truncate(PREVIEW_BYTES);
    if truncated
        && let Err(error) = std::str::from_utf8(&bytes)
        && error.error_len().is_none()
    {
        bytes.truncate(error.valid_up_to());
    }
    let mut text = String::from_utf8(bytes).context("样例预览不是 UTF-8 文本")?;
    if truncated {
        text.push_str("\n…预览已截断，运行使用完整文件");
    }
    Ok(text)
}
/// Detect changes from another editor instead of silently replacing them. New
/// files use create-only atomic commit; existing files preserve permissions.
pub fn save_text(path: &Path, original: Option<&TextFile>, text: &str) -> Result<TextFile> {
    ensure!(
        text.len() as u64 <= MAX_EDITOR_BYTES,
        "文本超过内置样例编辑器上限"
    );
    if let Some(original) = original {
        let current = read_text(path, MAX_EDITOR_BYTES)?;
        ensure!(
            &current == original,
            "文件已在其他窗口修改，请重新打开后再保存"
        );
        if current.text != text {
            crate::atomic_file::replace(path, text.as_bytes())?;
        }
    } else {
        ensure!(
            crate::atomic_file::create(path, text.as_bytes())?,
            "目标文件已存在，请重新选择文件"
        );
    }
    read_text(path, MAX_EDITOR_BYTES)
}
#[derive(Clone)]
pub struct ResolvedTest {
    pub index: usize,
    pub input: Arc<str>,
    pub expected_output: Option<Arc<str>>,
}
pub fn resolve_tests(
    document: &CodeDocument,
    selected: Option<usize>,
    limit: u64,
) -> Result<Vec<ResolvedTest>> {
    let mut remaining = limit;
    let mut resolve = |inline: &str, file: Option<&Path>| -> Result<Arc<str>> {
        let text = if let Some(file) = file {
            read_text(&resolve_path(document, file)?, remaining)?.text
        } else {
            ensure!(
                inline.len() as u64 <= remaining,
                "本次测试数据超过总上限（{limit} 字节）"
            );
            inline.to_owned()
        };
        remaining -= text.len() as u64;
        Ok(Arc::from(text))
    };
    document
        .metadata
        .tests
        .iter()
        .enumerate()
        .filter(|(index, _)| selected.is_none_or(|selected| *index == selected))
        .map(|(index, test)| {
            Ok(ResolvedTest {
                index,
                input: resolve(&test.input, test.input_file.as_deref())
                    .with_context(|| format!("用例 {} 输入读取失败", index + 1))?,
                expected_output: if test.expected_file.is_some() || test.expected_output.is_some() {
                    Some(
                        resolve(
                            test.expected_output.as_deref().unwrap_or_default(),
                            test.expected_file.as_deref(),
                        )
                        .with_context(|| format!("用例 {} 预期输出读取失败", index + 1))?,
                    )
                    .filter(|text| !text.is_empty())
                } else {
                    None
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bound_files_are_snapshots_and_save_as_keeps_their_targets() -> Result<()> {
        let root = std::env::temp_dir().join(format!("altaed-samples-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root)?;
        fs::write(root.join("input.txt"), "中文\n1 2\n")?;
        fs::write(root.join("answer.txt"), "3\n")?;
        let mut document = CodeDocument::untitled();
        document.path = Some(root.join("code.cpp"));
        document.metadata.settings.execution.checker =
            crate::checker::Checker::Executable(crate::checker::ExecutableChecker {
                program: "checker".into(),
                ..Default::default()
            });
        document.metadata.tests.push(crate::document::TestCase {
            input: "inline fallback".into(),
            expected_output: Some(String::new()),
            input_file: Some("input.txt".into()),
            expected_file: Some("answer.txt".into()),
        });
        let snapshot = resolve_tests(&document, None, 1024)?;
        fs::write(root.join("answer.txt"), "later edit\n")?;
        assert_eq!(&*snapshot[0].input, "中文\n1 2\n");
        assert_eq!(snapshot[0].expected_output.as_deref(), Some("3\n"));
        assert_eq!(document.metadata.tests[0].input, "inline fallback");
        crate::storage::save_document(&mut document)?;
        let restored = crate::storage::open_document(document.path.as_deref().unwrap())?;
        assert_eq!(
            restored.metadata.tests[0].input_file.as_deref(),
            Some(Path::new("input.txt"))
        );
        assert_eq!(
            resolve_tests(&restored, None, 1024)?[0]
                .expected_output
                .as_deref(),
            Some("later edit\n")
        );
        let copy = document.fork(root.join("other/copy.cpp"));
        let crate::checker::Checker::Executable(checker) =
            &copy.metadata.settings.execution.checker
        else {
            panic!("checker must survive save-as")
        };
        assert_eq!(
            copy.resolve_attachment_path(&checker.program)?,
            root.join("checker")
        );
        assert_eq!(
            restored.metadata.settings.execution,
            document.metadata.settings.execution
        );
        assert_eq!(
            resolve_path(&copy, copy.metadata.tests[0].input_file.as_deref().unwrap())?,
            root.join("input.txt")
        );
        assert!(resolve_tests(&document, None, 2).is_err());
        fs::remove_file(root.join("answer.txt"))?;
        assert!(resolve_tests(&document, None, 1024).is_err());
        document.metadata.tests[0].expected_output = None;
        assert!(
            resolve_tests(&document, None, 1024).is_err(),
            "binding is authoritative even for legacy disabled comparison"
        );
        fs::write(root.join("answer.txt"), "")?;
        assert!(
            resolve_tests(&document, None, 1024)?[0]
                .expected_output
                .is_none()
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn plain_text_save_detects_external_changes_and_does_not_overwrite_new_files() -> Result<()> {
        let root = std::env::temp_dir().join(format!("altaed-text-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root)?;
        let path = root.join("case.txt");
        let initial = save_text(&path, None, "original\n")?;
        assert!(save_text(&path, None, "unexpected").is_err());
        fs::write(&path, "external edit\n")?;
        assert!(save_text(&path, Some(&initial), "lost edit").is_err());
        assert_eq!(fs::read_to_string(&path)?, "external edit\n");
        let current = read_text(&path, MAX_EDITOR_BYTES)?;
        let saved = save_text(&path, Some(&current), "修改后\n")?;
        assert_eq!(saved.text, "修改后\n");
        fs::remove_dir_all(root)?;
        Ok(())
    }
}

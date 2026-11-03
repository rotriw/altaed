//! clangd's documented per-file compilation-command extension. Every document
//! receives a private URI, so two solutions never overwrite each other's text
//! or flags, including before the user has chosen a disk filename.
use crate::{
    document::{CodeDocument, DocumentId},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::{Path, PathBuf};

const SHADOW_SOURCE_NAME: &str = "document.cpp";
const FALLBACK_CPP_DRIVER: &str = "clang++";
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileCommand {
    pub working_directory: PathBuf,
    pub compilation_command: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentPlan {
    pub id: DocumentId,
    pub source: PathBuf,
    pub command: CompileCommand,
}
impl DocumentPlan {
    pub fn build(
        document: &CodeDocument,
        root: &Path,
        default_compiler: Option<&Path>,
    ) -> Result<Self> {
        ensure!(root.is_absolute(), "语言服务数据目录必须是绝对路径");
        let source = root.join(document.id.to_string()).join(SHADOW_SOURCE_NAME);
        let settings = &document.metadata.settings;
        let working = settings
            .working_directory
            .clone()
            .or_else(|| {
                document
                    .path
                    .as_ref()
                    .and_then(|path| path.parent().map(Path::to_owned))
            })
            .map(Ok)
            .unwrap_or_else(std::env::current_dir)?;
        let working_directory = std::fs::canonicalize(&working)
            .with_context(|| format!("语言服务工作目录不可用：{}", working.display()))?;
        let discovered = toolchain::discover_compiler();
        let compiler = settings
            .compiler_path
            .as_deref()
            .or(default_compiler)
            .or(discovered.as_deref())
            .unwrap_or_else(|| Path::new(FALLBACK_CPP_DRIVER));
        let text_path = |path: &Path| {
            path.to_str()
                .map(str::to_owned)
                .with_context(|| format!("语言服务路径不是 UTF-8：{}", path.display()))
        };
        let resolved = toolchain::find_executable(compiler);
        let mut arguments = vec![text_path(resolved.as_deref().unwrap_or(compiler))?];
        arguments.extend(settings.compiler_arguments.iter().cloned());
        if let Some(parent) = document.path.as_ref().and_then(|path| path.parent()) {
            // Shadow files keep document identity private, while quoted local
            // headers still resolve beside the user's actual source file.
            arguments.push("-iquote".into());
            arguments.push(text_path(parent)?);
        }
        arguments.push(text_path(&source)?);
        Ok(Self {
            id: document.id,
            source,
            command: CompileCommand {
                working_directory,
                compilation_command: arguments,
            },
        })
    }
    pub fn uri(&self) -> Result<lsp::Uri> {
        lsp::Uri::from_file_path(&self.source)
            .map_err(|()| anyhow::anyhow!("无法为文档 {} 创建语言服务 URI", self.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsaved_and_forked_documents_have_isolated_commands() -> Result<()> {
        let root = std::env::temp_dir().join(format!("altaed-lsp-plan-{}", uuid::Uuid::new_v4()));
        let mut first = CodeDocument::untitled();
        first.metadata.settings.compiler_arguments = vec!["-std=c++20".into(), "-DFIRST=1".into()];
        let second = first.fork(std::env::temp_dir().join("second.cpp"));
        let first_plan = DocumentPlan::build(&first, &root, Some(Path::new("clang++")))?;
        let second_plan = DocumentPlan::build(&second, &root, Some(Path::new("clang++")))?;
        assert_ne!(first_plan.uri()?, second_plan.uri()?);
        assert!(first.path.is_none());
        assert!(
            first_plan
                .command
                .compilation_command
                .contains(&"-DFIRST=1".into())
        );
        assert!(
            second_plan
                .command
                .compilation_command
                .contains(&"-iquote".into())
        );
        assert!(
            !root.exists(),
            "planning must not materialize or save source files"
        );
        let wire = serde_json::to_value(&first_plan.command)?;
        assert!(wire.get("workingDirectory").is_some());
        assert!(wire.get("compilationCommand").is_some());
        Ok(())
    }
}

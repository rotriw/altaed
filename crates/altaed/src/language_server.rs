//! Persisted language-service options. The editor adapter owns the protocol
//! process and provider objects; document storage stays independent of GPUI.
pub(crate) mod clangd_config;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CLANGD_PROGRAM: &str = "clangd";
const MAX_TIMEOUT_MS: u64 = 300_000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InlayHintSettings {
    pub enabled: bool,
    pub parameters: bool,
    pub types: bool,
}
impl Default for InlayHintSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            parameters: true,
            types: true,
        }
    }
}
// clangd's automatic fallback is lexical while its first preamble is building.
// Prefer semantic results and explicit overloads for reliable snippet arguments.
// Advanced user arguments with the same option name always take precedence.
const COMPLETION_DEFAULT_ARGUMENTS: &[&str] = &[
    "--completion-parse=always",
    "--completion-style=detailed",
    "--function-arg-placeholders=true",
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LanguageServerSettings {
    pub enabled: bool,
    pub path: Option<PathBuf>,
    pub arguments: Vec<String>,
    pub startup_timeout_ms: u64,
    pub request_timeout_ms: u64,
}
impl Default for LanguageServerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            path: None,
            arguments: [
                "--background-index".into(),
                "--header-insertion=never".into(),
                "--log=error".into(),
            ]
            .into_iter()
            .chain(
                COMPLETION_DEFAULT_ARGUMENTS
                    .iter()
                    .map(|argument| (*argument).to_owned()),
            )
            .collect(),
            startup_timeout_ms: 15_000,
            request_timeout_ms: 5_000,
        }
    }
}
impl LanguageServerSettings {
    pub fn effective_arguments(&self) -> Vec<String> {
        let mut arguments = self.arguments.clone();
        for default in COMPLETION_DEFAULT_ARGUMENTS {
            let name = default.split('=').next().unwrap_or(default);
            if !arguments
                .iter()
                .any(|argument| argument == name || argument.starts_with(&format!("{name}=")))
            {
                arguments.push((*default).to_owned());
            }
        }
        arguments
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_TIMEOUT_MS).contains(&self.startup_timeout_ms)
                && (1..=MAX_TIMEOUT_MS).contains(&self.request_timeout_ms),
            "语言服务超时必须在 1 到 {MAX_TIMEOUT_MS} 毫秒之间"
        );
        ensure!(
            self.path
                .as_ref()
                .is_none_or(|path| !path.as_os_str().is_empty()),
            "语言服务路径不能为空；自动查找请使用 null"
        );
        ensure!(
            !self
                .arguments
                .iter()
                .any(|argument| argument.starts_with("--compile-commands-dir")),
            "编译数据库由 IDE 按文档生成，请在文档运行配置中修改编译参数"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_defaults_upgrade_older_configuration_but_respect_explicit_options() {
        let settings = LanguageServerSettings {
            arguments: vec![
                "--completion-parse=auto".into(),
                "--function-arg-placeholders=false".into(),
            ],
            ..Default::default()
        };
        let arguments = settings.effective_arguments();
        assert!(arguments.contains(&"--completion-style=detailed".into()));
        assert!(!arguments.contains(&"--completion-parse=always".into()));
        assert!(!arguments.contains(&"--function-arg-placeholders=true".into()));
    }
}

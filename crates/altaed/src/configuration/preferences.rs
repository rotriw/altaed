//! Application-wide preferences. Document execution settings are stored separately.
use crate::{
    configuration::commands::{self, CommandId, Shortcuts},
    configuration::schema::Values,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPreferences {
    pub editor: EditorPreferences,
    pub toolchain: ToolchainPreferences,
    pub language_server: crate::language_server::LanguageServerSettings,
    pub workbench: WorkbenchPreferences,
    /// Keep namespaced plugin values even while their provider is unavailable.
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub keybindings: Shortcuts,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolchainPreferences {
    pub compiler_path: Option<std::path::PathBuf>,
    pub setup_complete: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorPreferences {
    pub font_family: String,
    pub font_size: f32,
    pub line_numbers: bool,
    pub soft_wrap: bool,
    pub auto_signature_help: bool,
}
impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            font_family: crate::platform::code_font().into(),
            font_size: 14.,
            line_numbers: true,
            soft_wrap: false,
            auto_signature_help: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkbenchPreferences {
    pub documents: bool,
    pub tests: bool,
    pub output: bool,
    pub reduce_motion: bool,
}
impl Default for WorkbenchPreferences {
    fn default() -> Self {
        Self {
            documents: false,
            tests: true,
            output: false,
            reduce_motion: false,
        }
    }
}
impl AppPreferences {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.editor.font_family.trim().is_empty(),
            "代码字体不能为空"
        );
        ensure!(
            self.editor.font_size.is_finite() && (8. ..=40.).contains(&self.editor.font_size),
            "字号必须在 8 到 40 像素之间"
        );
        self.language_server.validate()?;
        commands::validate_overrides(&self.keybindings)?;
        Ok(())
    }
    pub fn values(&self) -> Values {
        let mut values: Values = [
            ("core.editor.font_family", self.editor.font_family.clone()),
            ("core.editor.font_size", self.editor.font_size.to_string()),
            (
                "core.editor.line_numbers",
                self.editor.line_numbers.to_string(),
            ),
            ("core.editor.soft_wrap", self.editor.soft_wrap.to_string()),
            (
                "core.editor.auto_signature_help",
                self.editor.auto_signature_help.to_string(),
            ),
            (
                "core.workbench.documents",
                self.workbench.documents.to_string(),
            ),
            ("core.workbench.tests", self.workbench.tests.to_string()),
            ("core.workbench.output", self.workbench.output.to_string()),
            (
                "core.workbench.reduce_motion",
                self.workbench.reduce_motion.to_string(),
            ),
        ]
        .into_iter()
        .map(|(id, value)| (id.into(), value))
        .collect();
        values.insert(
            "core.toolchain.compiler_path".into(),
            self.toolchain
                .compiler_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        values.extend([
            (
                "core.lsp.enabled".into(),
                self.language_server.enabled.to_string(),
            ),
            (
                "core.lsp.path".into(),
                self.language_server
                    .path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            (
                "core.lsp.arguments".into(),
                self.language_server.arguments.join("\n"),
            ),
            (
                "core.lsp.startup_timeout_ms".into(),
                self.language_server.startup_timeout_ms.to_string(),
            ),
            (
                "core.lsp.request_timeout_ms".into(),
                self.language_server.request_timeout_ms.to_string(),
            ),
        ]);
        values.extend(self.extensions.iter().map(|(id, value)| {
            (
                id.clone(),
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            )
        }));
        values.extend(
            self.keybindings
                .iter()
                .map(|(id, shortcut)| (format!("core.keybindings.{}", id.0), shortcut.clone())),
        );
        values
    }
    pub fn apply(&self, values: &Values) -> Result<Self> {
        let value = |id: &str| values.get(id).with_context(|| format!("缺少设置：{id}"));
        let mut next = self.clone();
        next.editor.font_family = value("core.editor.font_family")?.trim().into();
        next.editor.font_size = value("core.editor.font_size")?.trim().parse()?;
        next.editor.line_numbers = value("core.editor.line_numbers")?.parse()?;
        next.editor.soft_wrap = value("core.editor.soft_wrap")?.parse()?;
        next.editor.auto_signature_help = value("core.editor.auto_signature_help")?.parse()?;
        next.workbench.documents = value("core.workbench.documents")?.parse()?;
        next.workbench.tests = value("core.workbench.tests")?.parse()?;
        next.workbench.output = value("core.workbench.output")?.parse()?;
        next.workbench.reduce_motion = value("core.workbench.reduce_motion")?.parse()?;
        let compiler = value("core.toolchain.compiler_path")?.trim();
        next.toolchain.compiler_path = (!compiler.is_empty()).then(|| compiler.into());
        next.language_server.enabled = value("core.lsp.enabled")?.parse()?;
        let server_path = value("core.lsp.path")?.trim();
        next.language_server.path = (!server_path.is_empty()).then(|| server_path.into());
        next.language_server.arguments = value("core.lsp.arguments")?
            .lines()
            .map(str::trim)
            .filter(|argument| !argument.is_empty())
            .map(str::to_owned)
            .collect();
        next.language_server.startup_timeout_ms =
            value("core.lsp.startup_timeout_ms")?.trim().parse()?;
        next.language_server.request_timeout_ms =
            value("core.lsp.request_timeout_ms")?.trim().parse()?;
        next.keybindings = values
            .iter()
            .filter_map(|(id, value)| {
                id.strip_prefix("core.keybindings.")
                    .map(|command| (CommandId(command.into()), value.trim().into()))
            })
            .collect();
        next.validate()?;
        Ok(next)
    }
}

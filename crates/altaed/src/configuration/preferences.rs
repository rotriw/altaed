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
    pub companion: crate::companion::CompanionSettings,
    pub editor: EditorPreferences,
    pub toolchain: ToolchainPreferences,
    pub language_server: crate::language_server::LanguageServerSettings,
    pub workbench: WorkbenchPreferences,
    /// Keep namespaced plugin values even while their provider is unavailable.
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub keybindings: Shortcuts,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolchainPreferences {
    pub default_cpp_standard: crate::toolchain::CppStandard,
    pub default_compiler_options: crate::toolchain::CompilerOptions,
    pub default_compiler_arguments: Vec<String>,
    pub compiler_path: Option<std::path::PathBuf>,
    pub setup_complete: bool,
}
impl Default for ToolchainPreferences {
    fn default() -> Self {
        let settings = crate::document::DocumentSettings::default();
        Self {
            default_cpp_standard: settings.cpp_standard,
            default_compiler_options: settings.compiler_options,
            default_compiler_arguments: settings.compiler_arguments,
            compiler_path: None,
            setup_complete: false,
        }
    }
}
impl ToolchainPreferences {
    /// New documents inherit a value snapshot; existing documents never follow
    /// subsequent changes to these defaults. The compiler path stays overridable.
    pub fn new_document_settings(&self) -> crate::document::DocumentSettings {
        crate::document::DocumentSettings {
            cpp_standard: self.default_cpp_standard,
            compiler_options: self.default_compiler_options.clone(),
            compiler_arguments: self.default_compiler_arguments.clone(),
            ..Default::default()
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorPreferences {
    pub theme: crate::color_theme::ColorThemeId,
    pub font_family: String,
    pub font_size: f32,
    pub line_numbers: bool,
    pub soft_wrap: bool,
    pub auto_signature_help: bool,
    pub auto_completions: bool,
    pub inlay_hints: crate::language_server::InlayHintSettings,
}
impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            theme: Default::default(),
            font_family: crate::platform::code_font().into(),
            font_size: 14.,
            line_numbers: true,
            soft_wrap: false,
            auto_signature_help: true,
            auto_completions: true,
            inlay_hints: Default::default(),
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
    pub compiling_color: Option<super::RgbColor>,
    pub running_color: Option<super::RgbColor>,
}
impl Default for WorkbenchPreferences {
    fn default() -> Self {
        Self {
            documents: false,
            tests: true,
            output: false,
            reduce_motion: false,
            compiling_color: None,
            running_color: None,
        }
    }
}
impl AppPreferences {
    pub fn validate(&self) -> Result<()> {
        self.editor.theme.definition()?;
        ensure!(
            !self.editor.font_family.trim().is_empty(),
            "代码字体不能为空"
        );
        ensure!(
            self.editor.font_size.is_finite() && (8. ..=40.).contains(&self.editor.font_size),
            "字号必须在 8 到 40 像素之间"
        );
        self.language_server.validate()?;
        self.companion.validate()?;
        commands::validate_overrides(&self.keybindings)?;
        Ok(())
    }
    pub fn values(&self) -> Values {
        let mut values: Values = [
            (
                "core.inlay.enabled",
                self.editor.inlay_hints.enabled.to_string(),
            ),
            (
                "core.inlay.parameters",
                self.editor.inlay_hints.parameters.to_string(),
            ),
            (
                "core.inlay.types",
                self.editor.inlay_hints.types.to_string(),
            ),
            ("core.import.enabled", self.companion.enabled.to_string()),
            ("core.import.port", self.companion.port.to_string()),
            ("core.editor.font_family", self.editor.font_family.clone()),
            ("core.editor.theme", self.editor.theme.value().into()),
            ("core.editor.font_size", self.editor.font_size.to_string()),
            (
                "core.editor.line_numbers",
                self.editor.line_numbers.to_string(),
            ),
            ("core.editor.soft_wrap", self.editor.soft_wrap.to_string()),
            (
                "core.editor.auto_completions",
                self.editor.auto_completions.to_string(),
            ),
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
                "core.workbench.compiling_color",
                self.workbench
                    .compiling_color
                    .map(|color| color.to_string())
                    .unwrap_or_default(),
            ),
            (
                "core.workbench.running_color",
                self.workbench
                    .running_color
                    .map(|color| color.to_string())
                    .unwrap_or_default(),
            ),
            (
                "core.workbench.reduce_motion",
                self.workbench.reduce_motion.to_string(),
            ),
        ]
        .into_iter()
        .map(|(id, value)| (id.into(), value))
        .collect();
        values.insert(
            "core.toolchain.cpp_standard".into(),
            self.toolchain.default_cpp_standard.value().into(),
        );
        values.extend(super::compiler::values(
            "core.toolchain",
            &self.toolchain.default_compiler_options,
        ));
        values.insert(
            "core.toolchain.arguments".into(),
            self.toolchain.default_compiler_arguments.join("\n"),
        );
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
        next.workbench.compiling_color =
            super::RgbColor::optional(value("core.workbench.compiling_color")?)?;
        next.workbench.running_color =
            super::RgbColor::optional(value("core.workbench.running_color")?)?;
        next.editor.inlay_hints.enabled = value("core.inlay.enabled")?.parse()?;
        next.editor.inlay_hints.parameters = value("core.inlay.parameters")?.parse()?;
        next.editor.inlay_hints.types = value("core.inlay.types")?.parse()?;
        next.companion.enabled = value("core.import.enabled")?.parse()?;
        next.companion.port = value("core.import.port")?.parse()?;
        next.editor.theme = crate::color_theme::ColorThemeId::parse(value("core.editor.theme")?)?;
        next.editor.font_family = value("core.editor.font_family")?.trim().into();
        next.editor.font_size = value("core.editor.font_size")?.trim().parse()?;
        next.editor.line_numbers = value("core.editor.line_numbers")?.parse()?;
        next.editor.soft_wrap = value("core.editor.soft_wrap")?.parse()?;
        next.editor.auto_completions = value("core.editor.auto_completions")?.parse()?;
        next.editor.auto_signature_help = value("core.editor.auto_signature_help")?.parse()?;
        next.workbench.documents = value("core.workbench.documents")?.parse()?;
        next.workbench.tests = value("core.workbench.tests")?.parse()?;
        next.workbench.output = value("core.workbench.output")?.parse()?;
        next.workbench.reduce_motion = value("core.workbench.reduce_motion")?.parse()?;
        next.toolchain.default_cpp_standard =
            crate::toolchain::CppStandard::parse(value("core.toolchain.cpp_standard")?)?;
        next.toolchain.default_compiler_options = super::compiler::apply("core.toolchain", values)?;
        next.toolchain.default_compiler_arguments = value("core.toolchain.arguments")?
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiler_defaults_are_snapshots_and_editor_hints_round_trip() -> Result<()> {
        let original = AppPreferences::default();
        assert!(original.editor.auto_completions);
        assert!(!original.editor.inlay_hints.enabled);
        let existing = original.toolchain.new_document_settings();
        let mut values = original.values();
        values.insert("core.toolchain.optimization".into(), "debug".into());
        values.insert("core.editor.theme".into(), "horizon-bright".into());
        values.insert("core.toolchain.debug_info".into(), "true".into());
        values.insert(
            "core.toolchain.arguments".into(),
            "-DLOCAL\n-Iinclude path".into(),
        );
        values.insert("core.inlay.parameters".into(), "false".into());
        values.insert("core.workbench.compiling_color".into(), "#123456".into());
        let next = original.apply(&values)?;
        assert_eq!(existing, crate::document::DocumentSettings::default());
        let mut new_document = next.toolchain.new_document_settings();
        assert_eq!(
            new_document.effective_compiler_arguments(),
            ["-DLOCAL", "-Iinclude path", "-Og", "-g"]
        );
        new_document.compiler_options.debug_info = false;
        assert!(next.toolchain.default_compiler_options.debug_info);
        assert_eq!(
            next.workbench.compiling_color.unwrap().to_string(),
            "#123456"
        );
        let mut invalid = values.clone();
        invalid.insert("core.workbench.compiling_color".into(), "red".into());
        assert!(original.apply(&invalid).is_err());
        assert!(!next.editor.inlay_hints.parameters && next.editor.inlay_hints.types);
        let decoded: AppPreferences = serde_json::from_slice(&serde_json::to_vec(&next)?)?;
        assert_eq!(decoded, next);
        assert_eq!(decoded.editor.theme.value(), "horizon-bright");
        values.insert("core.editor.theme".into(), "unknown-theme".into());
        assert!(original.apply(&values).is_err());
        Ok(())
    }
}

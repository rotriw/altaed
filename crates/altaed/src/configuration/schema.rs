//! Declarative settings shared by the web reference and native form.
//! Registration is independent of GPUI and of a future plugin runtime.
use crate::{checker::Checker, document::DocumentSettings};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub type Values = BTreeMap<String, String>;

#[derive(Clone, Deserialize)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub sidebar: f32,
    pub footer: f32,
    pub content_padding: f32,
    pub section_header: f32,
    pub control_height: f32,
    pub multiline_height: f32,
    pub row_gap: f32,
}

#[derive(Clone, Deserialize)]
pub struct Category {
    pub id: String,
    pub label: String,
    pub description: String,
    #[serde(default)]
    pub source: Option<String>,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Text,
    Multiline,
    Integer,
    Decimal,
    Choice,
    Toggle,
    Color,
}
#[derive(Clone, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
}
#[derive(Clone, Deserialize)]
pub struct Condition {
    pub id: String,
    pub value: String,
}
#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChoicePresentation {
    #[default]
    Buttons,
    Dropdown,
}
#[derive(Clone, Deserialize)]
pub struct Setting {
    pub id: String,
    pub category: String,
    pub label: String,
    pub description: String,
    pub kind: Kind,
    #[serde(default)]
    pub presentation: ChoicePresentation,
    #[serde(rename = "default")]
    pub default_value: serde_json::Value,
    #[serde(default)]
    pub placeholder: String,
    pub unit: Option<String>,
    pub control_width: Option<f32>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<Choice>,
    pub visible_when: Option<Condition>,
}
impl Setting {
    pub fn default_text(&self) -> String {
        match &self.default_value {
            serde_json::Value::String(value) => value.clone(),
            value => value.to_string(),
        }
    }
    pub fn visible(&self, values: &Values) -> bool {
        self.visible_when
            .as_ref()
            .is_none_or(|condition| values.get(&condition.id) == Some(&condition.value))
    }
    pub fn validate(&self, value: &str) -> Result<()> {
        ensure!(
            !self.required || !value.trim().is_empty(),
            "{}不能为空",
            self.label
        );
        let invalid = || {
            format!(
                "{}：请输入不小于 {} 的{}",
                self.label,
                self.min.unwrap_or(0.),
                if self.kind == Kind::Integer {
                    "整数"
                } else {
                    "有限数值"
                }
            )
        };
        match self.kind {
            Kind::Color => {
                if !value.trim().is_empty() {
                    super::RgbColor::parse(value)?;
                }
            }
            Kind::Integer => {
                let number = value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| anyhow::anyhow!(invalid()))?;
                ensure!(number as f64 >= self.min.unwrap_or(0.), "{}", invalid());
                ensure!(
                    self.max.is_none_or(|max| number as f64 <= max),
                    "{}不能大于 {}",
                    self.label,
                    self.max.unwrap_or_default()
                );
            }
            Kind::Decimal => {
                let number = value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| anyhow::anyhow!(invalid()))?;
                ensure!(
                    number.is_finite() && number >= self.min.unwrap_or(0.),
                    "{}",
                    invalid()
                );
                ensure!(
                    self.max.is_none_or(|max| number <= max),
                    "{}不能大于 {}",
                    self.label,
                    self.max.unwrap_or_default()
                );
            }
            Kind::Choice => ensure!(
                self.options.iter().any(|option| option.value == value),
                "{}：请选择一个有效选项",
                self.label
            ),
            Kind::Toggle => ensure!(
                matches!(value, "true" | "false"),
                "{}：开关值无效",
                self.label
            ),
            Kind::Text | Kind::Multiline => (),
        }
        Ok(())
    }
}
#[derive(Clone, Deserialize)]
pub struct Contribution {
    pub owner: String,
    pub categories: Vec<Category>,
    pub settings: Vec<Setting>,
}
#[derive(Deserialize)]
struct Builtins {
    layout: Layout,
    #[serde(flatten)]
    contribution: Contribution,
}
pub struct Registry {
    pub layout: Layout,
    pub categories: Vec<Category>,
    pub settings: Vec<Setting>,
}
impl Registry {
    pub fn builtins() -> Result<Self> {
        Self::from_json(include_str!("../../settings/document-schema.json"))
    }
    pub fn application() -> Result<Self> {
        let mut registry = Self::from_json(include_str!("../../settings/application-schema.json"))?;
        if let Some(font) = registry
            .settings
            .iter_mut()
            .find(|setting| setting.id == "core.editor.font_family")
        {
            font.default_value = crate::platform::code_font().into();
            font.placeholder = crate::platform::code_font().into();
        }
        registry.categories.push(Category {
            id: "core.keybindings".into(),
            label: "快捷键".into(),
            description: "点击“应用”后生效；留空可停用对应快捷键。".into(),
            source: None,
        });
        for command in crate::configuration::commands::definitions()? {
            registry.settings.push(Setting {
                id: command.setting_id(),
                category: "core.keybindings".into(),
                label: command.label.clone(),
                description:
                    "使用 cmd / ctrl / alt / shift 组合，多个按键序列以空格分隔；留空停用。".into(),
                kind: Kind::Text,
                default_value: serde_json::Value::String(command.default_shortcut().into()),
                placeholder: "未分配".into(),
                unit: None,
                control_width: Some(320.),
                min: None,
                max: None,
                required: false,
                options: Vec::new(),
                visible_when: None,
                presentation: ChoicePresentation::Buttons,
            });
        }
        Ok(registry)
    }
    fn from_json(json: &str) -> Result<Self> {
        let mut builtins: Builtins = serde_json::from_str(json)?;
        for setting in &mut builtins.contribution.settings {
            if matches!(
                setting.id.as_str(),
                "core.toolchain.optimization" | "core.compiler.optimization"
            ) {
                setting.options = crate::toolchain::Optimization::ALL
                    .into_iter()
                    .map(|optimization| Choice {
                        value: optimization.value().into(),
                        label: optimization.argument().unwrap_or("沿用附加参数").into(),
                    })
                    .collect();
            }
            if matches!(
                setting.id.as_str(),
                "core.toolchain.cpp_standard" | "core.compiler.standard"
            ) {
                setting.options = crate::toolchain::CppStandard::ALL
                    .into_iter()
                    .map(|standard| Choice {
                        value: standard.value().into(),
                        label: standard.label().into(),
                    })
                    .collect();
            }
        }
        let mut registry = Self {
            layout: builtins.layout,
            categories: Vec::new(),
            settings: Vec::new(),
        };
        registry.register(builtins.contribution)?;
        Ok(registry)
    }
    /// Validate the whole contribution before mutating the registry. Owners cannot
    /// shadow another provider's values, including settings from disabled plugins.
    pub fn register(&mut self, mut contribution: Contribution) -> Result<()> {
        let owner = &contribution.owner;
        ensure!(
            owner == "core" || (owner.starts_with("plugin.") && owner.len() > 7),
            "设置提供者必须使用 core 或 plugin.<标识> 命名空间"
        );
        let prefix = format!("{owner}.");
        let mut categories: BTreeSet<_> = self
            .categories
            .iter()
            .map(|category| category.id.as_str())
            .collect();
        for category in &contribution.categories {
            ensure!(
                category.id.starts_with(&prefix) && categories.insert(&category.id),
                "设置分类重复或不属于提供者：{}",
                category.id
            );
        }
        let mut ids: BTreeSet<_> = self
            .settings
            .iter()
            .map(|setting| setting.id.as_str())
            .collect();
        for setting in &contribution.settings {
            ensure!(
                !setting.label.trim().is_empty(),
                "设置名称不能为空：{}",
                setting.id
            );
            ensure!(
                setting
                    .control_width
                    .is_none_or(|width| width.is_finite() && width > 0.),
                "控件宽度无效：{}",
                setting.id
            );
            ensure!(
                setting.id.starts_with(&prefix) && ids.insert(&setting.id),
                "设置标识重复或不属于提供者：{}",
                setting.id
            );
            ensure!(
                setting.category.starts_with(&prefix)
                    && categories.contains(setting.category.as_str()),
                "设置引用了无效分类：{}",
                setting.id
            );
            // Conditional paths/secrets have no useful default until enabled.
            // Once visible they undergo the same required-field validation.
            if !(setting.visible_when.is_some()
                && setting.required
                && setting.default_text().trim().is_empty())
            {
                setting.validate(&setting.default_text())?;
            }
            if setting.kind == Kind::Choice {
                let choices: BTreeSet<_> =
                    setting.options.iter().map(|choice| &choice.value).collect();
                ensure!(
                    choices.len() == setting.options.len(),
                    "选择项存在重复值：{}",
                    setting.id
                );
            }
        }
        // Conditions only depend on unconditional fields from the same provider:
        // no cycles, hidden dependency chains, or cross-plugin lifecycle coupling.
        for setting in &contribution.settings {
            if let Some(condition) = &setting.visible_when {
                let dependency = contribution
                    .settings
                    .iter()
                    .chain(&self.settings)
                    .find(|candidate| candidate.id == condition.id)
                    .context("条件字段不存在")?;
                ensure!(
                    dependency.id.starts_with(&prefix)
                        && dependency.visible_when.is_none()
                        && dependency.id != setting.id,
                    "条件字段依赖无效：{}",
                    setting.id
                );
                dependency.validate(&condition.value)?;
            }
        }
        if let Some(plugin) = owner.strip_prefix("plugin.") {
            for category in &mut contribution.categories {
                category.source.get_or_insert_with(|| plugin.to_owned());
            }
        }
        self.categories.extend(contribution.categories);
        self.settings.extend(contribution.settings);
        Ok(())
    }
    pub fn values(&self, settings: &DocumentSettings) -> Values {
        let mut values: Values = self
            .settings
            .iter()
            .map(|setting| (setting.id.clone(), setting.default_text()))
            .collect();
        let core = [
            (
                "core.compiler.standard",
                settings.cpp_standard.value().into(),
            ),
            (
                "core.compiler.path",
                settings
                    .compiler_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
            ),
            (
                "core.compiler.arguments",
                settings.compiler_arguments.join("\n"),
            ),
            (
                "core.compiler.directory",
                settings
                    .working_directory
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
            ),
            (
                "core.execution.compile_timeout",
                settings.execution.compile_timeout_ms.to_string(),
            ),
            (
                "core.execution.test_timeout",
                settings.execution.test_timeout_ms.to_string(),
            ),
            (
                "core.execution.output_limit",
                settings.execution.output_limit_bytes.to_string(),
            ),
            (
                "core.execution.test_data_limit",
                settings.execution.test_data_limit_bytes.to_string(),
            ),
            (
                "core.checker.mode",
                settings.execution.checker.mode_id().into(),
            ),
        ];
        values.extend(core.into_iter().map(|(id, value)| (id.into(), value)));
        values.extend(super::compiler::values(
            "core.compiler",
            &settings.compiler_options,
        ));
        if let Checker::Float { absolute, relative } = settings.execution.checker {
            values.insert("core.checker.absolute".into(), absolute.to_string());
            values.insert("core.checker.relative".into(), relative.to_string());
        }
        if let Checker::Executable(checker) = &settings.execution.checker {
            values.insert(
                "core.checker.program".into(),
                checker.program.display().to_string(),
            );
            values.insert(
                "core.checker.timeout".into(),
                checker.timeout_ms.to_string(),
            );
            values.insert(
                "core.checker.arguments".into(),
                checker.arguments.join("\n"),
            );
        }
        for (id, value) in &settings.extensions {
            if self.settings.iter().any(|setting| &setting.id == id) {
                values.insert(
                    id.clone(),
                    if let Some(text) = value.as_str() {
                        text.to_owned()
                    } else {
                        value.to_string()
                    },
                );
            }
        }
        values
    }
    pub fn errors(&self, values: &Values) -> BTreeMap<String, String> {
        self.settings
            .iter()
            .filter(|setting| setting.visible(values))
            .filter_map(|setting| {
                let result = values
                    .get(&setting.id)
                    .context("缺少设置值")
                    .and_then(|value| setting.validate(value));
                result
                    .err()
                    .map(|error| (setting.id.clone(), error.to_string()))
            })
            .collect()
    }
    pub fn apply(&self, original: &DocumentSettings, values: &Values) -> Result<DocumentSettings> {
        ensure!(self.errors(values).is_empty(), "请修正标记的设置");
        let value = |id: &str| values.get(id).with_context(|| format!("缺少设置：{id}"));
        let mut settings = original.clone();
        settings.cpp_standard =
            crate::toolchain::CppStandard::parse(value("core.compiler.standard")?)?;
        settings.compiler_options = super::compiler::apply("core.compiler", values)?;
        settings.compiler_path = (!value("core.compiler.path")?.trim().is_empty())
            .then(|| value("core.compiler.path").map(|path| path.trim().into()))
            .transpose()?;
        settings.compiler_arguments = value("core.compiler.arguments")?
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        settings.working_directory = (!value("core.compiler.directory")?.trim().is_empty())
            .then(|| value("core.compiler.directory").map(|path| path.trim().into()))
            .transpose()?;
        settings.execution.compile_timeout_ms =
            value("core.execution.compile_timeout")?.trim().parse()?;
        settings.execution.test_timeout_ms =
            value("core.execution.test_timeout")?.trim().parse()?;
        settings.execution.output_limit_bytes =
            value("core.execution.output_limit")?.trim().parse()?;
        settings.execution.test_data_limit_bytes =
            value("core.execution.test_data_limit")?.trim().parse()?;
        settings.execution.checker = match value("core.checker.mode")?.as_str() {
            "float" => Checker::Float {
                absolute: value("core.checker.absolute")?.trim().parse()?,
                relative: value("core.checker.relative")?.trim().parse()?,
            },
            "executable" => Checker::Executable(crate::checker::ExecutableChecker {
                program: value("core.checker.program")?.trim().into(),
                timeout_ms: value("core.checker.timeout")?.trim().parse()?,
                arguments: value("core.checker.arguments")?
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect(),
            }),
            id => Checker::builtins()
                .into_iter()
                .find(|(mode, _)| *mode == id)
                .map(|(_, checker)| checker)
                .context("比较方式无效")?,
        };
        self.apply_extensions(values, &mut settings.extensions)?;
        settings.execution.validate()?;
        Ok(settings)
    }
    fn apply_extensions(
        &self,
        values: &Values,
        extensions: &mut BTreeMap<String, serde_json::Value>,
    ) -> Result<()> {
        for setting in &self.settings {
            if setting.id.starts_with("plugin.") && setting.visible(values) {
                let text = values.get(&setting.id).context("缺少插件设置")?;
                let typed = match setting.kind {
                    Kind::Toggle => serde_json::Value::Bool(text == "true"),
                    Kind::Integer => serde_json::Value::from(text.trim().parse::<u64>()?),
                    Kind::Decimal => serde_json::Value::from(text.trim().parse::<f64>()?),
                    _ => serde_json::Value::String(text.clone()),
                };
                extensions.insert(setting.id.clone(), typed);
            }
        }
        Ok(())
    }
    pub fn application_values(&self, settings: &crate::configuration::AppPreferences) -> Values {
        let mut values: Values = self
            .settings
            .iter()
            .map(|setting| (setting.id.clone(), setting.default_text()))
            .collect();
        values.extend(settings.values());
        values
    }
    pub fn apply_application(
        &self,
        original: &crate::configuration::AppPreferences,
        values: &Values,
    ) -> Result<crate::configuration::AppPreferences> {
        ensure!(self.errors(values).is_empty(), "请修正标记的设置");
        let mut next = original.apply(values)?;
        self.apply_extensions(values, &mut next.extensions)?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contributions_validate_atomically_and_preserve_unloaded_values() -> Result<()> {
        let mut registry = Registry::builtins()?;
        let extension: Contribution = serde_json::from_value(serde_json::json!({
            "owner":"plugin.submit", "categories":[{"id":"plugin.submit.channel","label":"提交","description":"测试扩展"}],
            "settings":[
                {"id":"plugin.submit.route","category":"plugin.submit.channel","label":"通道","description":"选择通道","kind":"choice","default":"direct","options":[{"value":"direct","label":"直连"},{"value":"relay","label":"中继"}]},
                {"id":"plugin.submit.confirm","category":"plugin.submit.channel","label":"提交前确认","description":"确认","kind":"toggle","default":true}
            ]
        }))?;
        registry.register(extension)?;
        let before = registry.settings.len();
        let bad: Contribution = serde_json::from_value(serde_json::json!({
            "owner":"plugin.invalid", "categories":[{"id":"plugin.invalid.settings","label":"无效","description":"无效"}],
            "settings":[{"id":"core.compiler.path","category":"plugin.invalid.settings","label":"覆盖","description":"不允许","kind":"text","default":""}]
        }))?;
        assert!(registry.register(bad).is_err());
        assert_eq!(registry.settings.len(), before);
        assert!(
            !registry
                .categories
                .iter()
                .any(|category| category.id == "plugin.invalid.settings")
        );
        let mut original = DocumentSettings::default();
        original.extensions.insert(
            "plugin.unloaded.secret".into(),
            serde_json::json!({"retained":true}),
        );
        let mut values = registry.values(&original);
        values.insert("plugin.submit.route".into(), "relay".into());
        values.insert("plugin.submit.confirm".into(), "false".into());
        let next = registry.apply(&original, &values)?;
        assert_eq!(next.extensions["plugin.submit.route"], "relay");
        assert_eq!(next.extensions["plugin.submit.confirm"], false);
        assert_eq!(
            next.extensions["plugin.unloaded.secret"],
            original.extensions["plugin.unloaded.secret"]
        );
        // The form edits a copy. An unrelated document and defaults are unaffected.
        assert!(!original.extensions.contains_key("plugin.submit.route"));
        values.insert("core.execution.test_timeout".into(), "0".into());
        assert!(registry.apply(&original, &values).is_err());
        values.insert("core.execution.test_timeout".into(), "2000".into());
        values.insert("core.checker.mode".into(), "float".into());
        values.insert("core.checker.absolute".into(), "NaN".into());
        assert!(registry.apply(&original, &values).is_err());
        values.insert("core.checker.mode".into(), "exact".into());
        assert!(
            registry.apply(&original, &values).is_ok(),
            "hidden float fields do not invalidate exact comparison"
        );
        values.insert("core.checker.mode".into(), "executable".into());
        assert!(
            registry.apply(&original, &values).is_err(),
            "an enabled native checker requires a path"
        );
        values.insert("core.checker.program".into(), "checker with spaces".into());
        values.insert(
            "core.checker.arguments".into(),
            "one argument\n--verbose".into(),
        );
        let next = registry.apply(&original, &values)?;
        assert!(
            matches!(&next.execution.checker, Checker::Executable(checker) if checker.arguments == ["one argument", "--verbose"])
        );
        let restored = registry.values(&next);
        for id in [
            "core.checker.mode",
            "core.checker.program",
            "core.checker.arguments",
            "core.checker.timeout",
        ] {
            assert_eq!(restored[id], values[id], "checker field round-trips: {id}");
        }
        Ok(())
    }
}

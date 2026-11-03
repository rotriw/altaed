//! Offline theme catalog and the adapter to the pinned editor's theme registry.
//! Identifiers belong to config.json; upstream display names belong to the UI.
use anyhow::{Context as _, Result, ensure};
use gpui::{App, BorrowAppContext};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::OnceLock};

pub const SETTING_ID: &str = "core.editor.theme";
pub const FAMILY_BYTES: &[u8] = include_bytes!("../assets/themes/bundled.json");
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ColorThemeId(String);
impl Default for ColorThemeId {
    fn default() -> Self {
        Self("default-light".into())
    }
}
impl ColorThemeId {
    pub fn parse(value: &str) -> Result<Self> {
        let id = Self(value.into());
        id.definition()?;
        Ok(id)
    }
    pub fn value(&self) -> &str {
        &self.0
    }
    pub fn definition(&self) -> Result<&'static ThemeDefinition> {
        catalog()?
            .themes
            .iter()
            .find(|theme| theme.id == self.0)
            .with_context(|| format!("未知颜色主题：{}", self.0))
    }
}
#[derive(Deserialize)]
pub struct Catalog {
    pub themes: Vec<ThemeDefinition>,
}
#[derive(Deserialize)]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    pub label: String,
    pub appearance: String,
    pub style: serde_json::Value,
}
pub fn catalog() -> Result<&'static Catalog> {
    static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| load_catalog().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| anyhow::anyhow!("内置主题数据无效：{error}"))
}
fn load_catalog() -> Result<Catalog> {
    let catalog: Catalog = serde_json::from_slice(FAMILY_BYTES)?;
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for theme in &catalog.themes {
        ensure!(
            ids.insert(&theme.id) && names.insert(&theme.name),
            "主题标识或名称重复"
        );
        ensure!(
            !theme.label.is_empty() && matches!(theme.appearance.as_str(), "light" | "dark"),
            "主题描述无效：{}",
            theme.id
        );
    }
    Ok(catalog)
}
pub fn apply(id: &ColorThemeId, cx: &mut App) -> Result<()> {
    let definition = id.definition()?;
    let registry = theme::ThemeRegistry::global(cx);
    ensure!(
        registry.get(&definition.name).is_ok(),
        "内置主题未注册：{}",
        definition.name
    );
    // This store is the editor adapter's in-memory settings, not the user's Zed
    // configuration. altaed persists its own typed preference separately.
    let settings = serde_json::json!({"theme":definition.name}).to_string();
    cx.update_global::<settings::SettingsStore, _>(|store, cx| {
        store.set_user_settings(&settings, cx).result()
    })?;
    theme::GlobalTheme::reload_theme(cx);
    Ok(())
}

#[derive(Deserialize)]
pub struct PreviewFragment {
    pub text: String,
    pub capture: Option<String>,
}
pub fn preview() -> Result<&'static Vec<Vec<PreviewFragment>>> {
    static PREVIEW: OnceLock<Result<Vec<Vec<PreviewFragment>>, String>> = OnceLock::new();
    PREVIEW
        .get_or_init(|| {
            serde_json::from_slice(include_bytes!("../assets/themes/preview.json"))
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!("主题预览无效：{error}"))
}

pub fn execution_color(
    id: &ColorThemeId,
    phase: altaed_core::runner::ExecutionPhase,
) -> Result<crate::configuration::RgbColor> {
    let definition = id.definition()?;
    let color = |key: &str| -> Result<crate::configuration::RgbColor> {
        let value = definition
            .style
            .get(key)
            .and_then(serde_json::Value::as_str)
            .context("主题缺少执行状态颜色")?;
        crate::configuration::RgbColor::parse(value.get(..7).context("主题颜色格式无效")?)
    };
    let background = color("editor.background")?;
    let accent = color(match phase {
        altaed_core::runner::ExecutionPhase::Compiling => "text.accent",
        altaed_core::runner::ExecutionPhase::Running => "success",
    })?;
    Ok(background.mix(accent, 12))
}

#[cfg(test)]
mod tests {
    use super::*;
    use theme::ActiveTheme;

    fn assert_color(actual: gpui::Hsla, expected: gpui::Hsla) {
        for (a, b) in [
            (actual.h, expected.h),
            (actual.s, expected.s),
            (actual.l, expected.l),
            (actual.a, expected.a),
        ] {
            assert!((a - b).abs() < 0.000001, "{actual:?} != {expected:?}");
        }
    }

    fn rgb_color(color: gpui::Hsla) -> crate::configuration::RgbColor {
        crate::configuration::RgbColor::new(u32::from(gpui::Rgba::from(color)) >> 8)
    }
    fn assert_ui_text_readable(theme: &theme::Theme) {
        let c = theme.colors();
        let surfaces = [
            c.editor_background,
            c.surface_background,
            c.elevated_surface_background,
            c.editor_background.blend(c.element_selected),
            c.surface_background.blend(c.element_selected),
            c.elevated_surface_background.blend(c.element_selected),
        ];
        for foreground in [
            c.text,
            c.text_muted,
            c.text_placeholder,
            c.text_accent,
            theme.status().error,
            theme.status().success,
            theme.status().warning,
        ] {
            for surface in surfaces {
                let ratio = rgb_color(foreground).contrast(rgb_color(surface));
                assert!(
                    ratio >= 4.5,
                    "{}: UI text contrast {ratio:.2} for {foreground:?} on {surface:?}",
                    theme.name
                );
            }
        }
    }

    // Exercise the actual pinned registry, not just our JSON DTO: unsupported
    // fields can deserialize successfully while falling back to base colors.
    #[gpui::test]
    fn bundled_themes_register_and_preserve_horizon_syntax(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            theme::init(
                theme::LoadThemes::All(Box::new(crate::app_assets::Assets)),
                cx,
            );
            let catalog = catalog().unwrap();
            assert_eq!(preview().unwrap().len(), 4);
            assert_eq!(catalog.themes.len(), 14);
            let schema = crate::configuration::schema::Registry::application().unwrap();
            let choices = &schema
                .settings
                .iter()
                .find(|item| item.id == "core.editor.theme")
                .unwrap()
                .options;
            assert_eq!(
                choices
                    .iter()
                    .map(|option| option.value.as_str())
                    .collect::<Vec<_>>(),
                catalog
                    .themes
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                catalog
                    .themes
                    .iter()
                    .filter(|theme| theme.id.starts_with("horizon"))
                    .count(),
                6
            );
            for definition in &catalog.themes {
                let id = ColorThemeId::parse(&definition.id).unwrap();
                apply(&id, cx).unwrap();
                assert_eq!(cx.theme().name.as_ref(), definition.name);
                assert_ui_text_readable(cx.theme());
                let expected: gpui::Hsla =
                    gpui::Rgba::try_from(definition.style["editor.background"].as_str().unwrap())
                        .unwrap()
                        .into();
                assert_color(cx.theme().colors().editor_background, expected);
                let syntax = cx.theme().syntax();
                for capture in [
                    "keyword",
                    "function",
                    "type.builtin",
                    "variable",
                    "number",
                    "string",
                    "comment",
                ] {
                    let expected: gpui::Hsla = gpui::Rgba::try_from(
                        definition.style["syntax"][capture]["color"]
                            .as_str()
                            .unwrap(),
                    )
                    .unwrap()
                    .into();
                    assert_color(syntax.get(capture).color.unwrap(), expected);
                }
                let keyword = syntax.get("keyword");
                if definition.id.ends_with("-italic") {
                    assert_eq!(keyword.font_style, Some(gpui::FontStyle::Italic));
                }
                if definition.id.ends_with("-bold") {
                    assert_eq!(keyword.font_weight, Some(gpui::FontWeight::BOLD));
                }
                for phase in [
                    altaed_core::runner::ExecutionPhase::Compiling,
                    altaed_core::runner::ExecutionPhase::Running,
                ] {
                    execution_color(&id, phase).unwrap();
                }
            }
            assert!(ColorThemeId::parse("not-a-theme").is_err());
            apply(&ColorThemeId::parse("horizon-bright").unwrap(), cx).unwrap();
            assert_color(
                cx.theme().colors().editor_background,
                gpui::rgb(0xfdf0ed).into(),
            );
            // VS Code composites this translucent comment over the editor
            // surface; GPUI would otherwise blend it over the foreground.
            let text = gpui::TextStyle {
                color: cx.theme().colors().editor_foreground,
                ..Default::default()
            }
            .highlight(cx.theme().syntax().get("comment"));
            assert_color(text.color, gpui::rgb(0x989190).into());
        });
    }
}

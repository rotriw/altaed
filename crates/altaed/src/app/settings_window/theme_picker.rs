//! Compact choices share one trigger. Theme choices add a lightweight syntax
//! preview using the same catalog and sample as the web review, without an editor.
use super::*;
use crate::color_theme;
use gpui::{AnyElement, StyledText, div, px};
use settings::Settings;
use ui::prelude::*;

impl SettingsWindow {
    pub(super) fn choice_dropdown(&self, setting: &Setting, cx: &Context<Self>) -> AnyElement {
        let palette = crate::panel_style::palette(cx);
        let value = self.values().get(&setting.id).cloned().unwrap_or_default();
        let label = setting
            .options
            .iter()
            .find(|option| option.value == value)
            .map(|option| option.label.clone())
            .unwrap_or_else(|| value.clone());
        let owner = cx.weak_entity();
        let id = setting.id.clone();
        let options = setting.options.clone();
        let is_theme = id == color_theme::SETTING_ID;
        let font_size = theme::ThemeSettings::get_global(cx).ui_font_size(cx);
        let content = div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(9.))
            .text_size(px(12.))
            .text_color(palette.text)
            .when(is_theme, |row| row.child(swatches(&value, cx)))
            .child(div().flex_1().min_w_0().text_left().truncate().child(label))
            .child(
                ui::Icon::new(ui::IconName::ChevronDown)
                    .size(ui::IconSize::Custom(gpui::rems(px(14.) / font_size)))
                    .color(ui::Color::Custom(palette.muted)),
            );
        let selector = div()
            .w_full()
            .max_w(px(setting.control_width.unwrap_or(280.)))
            .h(px(30.))
            .border_1()
            .border_color(palette.line)
            .rounded(px(2.))
            .child(
                ui::PopoverMenu::new(format!("select-{}", setting.id))
                    .full_width(true)
                    .trigger(
                        ui::ButtonLike::new(format!("select-trigger-{}", setting.id))
                            .style(ui::ButtonStyle::Transparent)
                            .size(ui::ButtonSize::None)
                            .height(px(28.).into())
                            .full_width()
                            .tab_index(0_isize)
                            .tooltip(ui::Tooltip::text(setting.label.clone()))
                            .child(content),
                    )
                    .menu(move |window, cx| {
                        let owner = owner.clone();
                        let id = id.clone();
                        let value = value.clone();
                        let options = options.clone();
                        Some(ui::ContextMenu::build(window, cx, move |mut menu, _, _| {
                            for option in options {
                                let owner = owner.clone();
                                let id = id.clone();
                                if is_theme {
                                    let selected = option.value == value;
                                    let theme_id = option.value.clone();
                                    let label = option.label.clone();
                                    menu = menu.custom_entry(
                                        move |_, cx| {
                                            let palette = crate::panel_style::palette(cx);
                                            div()
                                                .min_w(px(250.))
                                                .h(px(28.))
                                                .flex()
                                                .items_center()
                                                .gap(px(10.))
                                                .text_size(px(12.))
                                                .child(swatches(&theme_id, cx))
                                                .child(div().flex_1().child(label.clone()))
                                                .child(div().w(px(14.)).when(selected, |slot| {
                                                    slot.child(
                                                        ui::Icon::new(ui::IconName::Check)
                                                            .size(ui::IconSize::Small)
                                                            .color(ui::Color::Custom(
                                                                palette.accent,
                                                            )),
                                                    )
                                                }))
                                                .into_any_element()
                                        },
                                        move |_, cx| {
                                            let _ = owner.update(cx, |this, cx| {
                                                if !this.saving {
                                                    this.set_value(
                                                        id.clone(),
                                                        option.value.clone(),
                                                        cx,
                                                    );
                                                }
                                            });
                                        },
                                    );
                                } else {
                                    menu = menu.toggleable_entry(
                                        option.label,
                                        option.value == value,
                                        ui::IconPosition::Start,
                                        None,
                                        move |_, cx| {
                                            let _ = owner.update(cx, |this, cx| {
                                                if !this.saving {
                                                    this.set_value(
                                                        id.clone(),
                                                        option.value.clone(),
                                                        cx,
                                                    );
                                                }
                                            });
                                        },
                                    );
                                }
                            }
                            menu
                        }))
                    })
                    .anchor(gpui::Corner::TopLeft),
            );
        let mut control = div().w_full().max_w(px(420.)).child(selector);
        if is_theme {
            let selected = self.values().get(&setting.id).cloned().unwrap_or_default();
            if let Some(preview) = syntax_preview(&selected, cx) {
                control = control.child(preview);
            }
        }
        control.into_any_element()
    }
}

fn registered_theme(id: &str, cx: &App) -> Option<Arc<theme::Theme>> {
    let id = color_theme::ColorThemeId::parse(id).ok()?;
    theme::ThemeRegistry::global(cx)
        .get(&id.definition().ok()?.name)
        .ok()
}
fn swatches(id: &str, cx: &App) -> AnyElement {
    let mut band = div().flex().items_center().flex_shrink_0().gap(px(2.));
    if let Some(theme) = registered_theme(id, cx) {
        for capture in ["keyword", "function", "string", "number"] {
            band = band.child(
                div()
                    .w(px(3.))
                    .h(px(12.))
                    .rounded(px(1.))
                    .bg(theme.syntax().color(capture)),
            );
        }
    }
    band.into_any_element()
}
fn syntax_preview(id: &str, cx: &App) -> Option<AnyElement> {
    let theme = registered_theme(id, cx)?;
    let lines = color_theme::preview().ok()?;
    let colors = theme.colors();
    let syntax = theme.syntax();
    let mut sample = div()
        .mt(px(10.))
        .w_full()
        .py(px(8.))
        .px(px(10.))
        .rounded(px(2.))
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .text_color(colors.editor_foreground)
        .font_family(
            theme::ThemeSettings::get_global(cx)
                .buffer_font
                .family
                .clone(),
        )
        .text_size(px(12.))
        .line_height(px(20.));
    for (index, fragments) in lines.iter().enumerate() {
        let mut text = String::new();
        let mut highlights = Vec::new();
        for fragment in fragments {
            let start = text.len();
            text.push_str(&fragment.text);
            if let Some(capture) = &fragment.capture {
                highlights.push((start..text.len(), syntax.get(capture)));
            }
        }
        sample = sample.child(
            div()
                .flex()
                .h(px(20.))
                .child(
                    div()
                        .w(px(24.))
                        .flex_shrink_0()
                        .text_color(colors.text_muted)
                        .child((index + 1).to_string()),
                )
                .child(StyledText::new(text).with_highlights(highlights)),
        );
    }
    Some(sample.into_any_element())
}

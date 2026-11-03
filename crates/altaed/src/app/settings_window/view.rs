use super::*;
use crate::panel_style::{self, form_button as button};
use gpui::{AnyElement, FontWeight, div};

impl SettingsWindow {
    fn field(
        &mut self,
        setting: &Setting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = panel_style::palette(cx);
        let error = self.errors.get(&setting.id).cloned();
        let mut row = div()
            .mb(px(self.registry.layout.row_gap))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .line_height(px(20.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(setting.label.clone()),
                    )
                    .when_some(setting.unit.clone(), |row, unit| {
                        row.child(
                            div()
                                .text_size(px(11.))
                                .text_color(palette.muted)
                                .child(unit),
                        )
                    }),
            )
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(12.))
                    .line_height(px(20.))
                    .text_color(palette.muted)
                    .child(setting.description.clone()),
            );
        let control = if setting.kind == Kind::Choice
            && setting.presentation == crate::configuration::schema::ChoicePresentation::Dropdown
        {
            self.choice_dropdown(setting, cx)
        } else if setting.kind == Kind::Choice {
            let mut choices = div().flex().flex_wrap().gap(px(8.));
            for choice in &setting.options {
                let id = setting.id.clone();
                let value = choice.value.clone();
                let selected = self.values().get(&id) == Some(&value);
                choices = choices.child(
                    button(
                        format!("{}-{}", id, value),
                        &choice.label,
                        self.registry.layout.control_height,
                        palette,
                    )
                    .min_w(px(80.))
                    .px(px(12.))
                    .border_color(if selected {
                        palette.accent
                    } else {
                        palette.line
                    })
                    .text_color(if selected {
                        palette.accent
                    } else {
                        palette.text
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.saving {
                            this.set_value(id.clone(), value.clone(), cx);
                        }
                    })),
                );
            }
            choices.into_any_element()
        } else if setting.kind == Kind::Toggle {
            let id = setting.id.clone();
            let enabled = self.values().get(&id).is_some_and(|value| value == "true");
            button(format!("{}-toggle", id), "", 28., palette)
                .w_full()
                .px(px(0.))
                .justify_start()
                .gap(px(7.))
                .child(
                    div()
                        .size(px(13.))
                        .flex_shrink_0()
                        .border_1()
                        .border_color(if enabled {
                            palette.accent
                        } else {
                            palette.muted
                        })
                        .when(enabled, |box_| {
                            box_.bg(palette.accent).child(
                                ui::Icon::new(ui::IconName::Check)
                                    .size(ui::IconSize::Custom(gpui::rems(
                                        px(11.) / window.rem_size(),
                                    )))
                                    .color(ui::Color::Custom(palette.paper)),
                            )
                        }),
                )
                .child("启用")
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.saving {
                        this.set_value(id.clone(), (!enabled).to_string(), cx);
                    }
                }))
                .into_any_element()
        } else {
            let field = self.ensure_input(setting, window, cx);
            let focus = field.focus_handle(cx);
            let input = div()
                .id(format!("input-{}", setting.id))
                .w_full()
                .when_some(setting.control_width, |input, width| input.max_w(px(width)))
                .h(px(if setting.kind == Kind::Multiline {
                    self.registry.layout.multiline_height
                } else {
                    self.registry.layout.control_height
                }))
                .border_1()
                .border_color(if error.is_some() {
                    palette.error
                } else {
                    palette.line
                })
                .track_focus(&focus)
                .tab_index(0_isize)
                .focus(|style| style.border_color(palette.accent))
                .child(field);
            if setting.kind == Kind::Color {
                let color = self
                    .values()
                    .get(&setting.id)
                    .and_then(|value| crate::configuration::RgbColor::parse(value).ok());
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(input)
                    .child(
                        div()
                            .size(px(28.))
                            .flex_shrink_0()
                            .border_1()
                            .border_color(palette.line)
                            .when_some(color, |swatch, color| swatch.bg(gpui::rgb(color.rgb()))),
                    )
                    .into_any_element()
            } else {
                input.into_any_element()
            }
        };
        row = row.child(div().mt(px(12.)).child(control));
        if let Some(error) = error {
            row = row.child(
                div()
                    .mt(px(6.))
                    .text_size(px(12.))
                    .line_height(px(20.))
                    .text_color(palette.error)
                    .child(error),
            );
        }
        row.into_any_element()
    }
}

impl SettingsWindow {
    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let left = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .child(self.scope.title())
            .when(self.dirty(), |row| {
                row.child(div().text_color(palette.muted).child("未应用"))
            });
        let right = div()
            .text_size(px(11.))
            .text_color(palette.muted)
            .child(self.scope.explanation());
        self.window_chrome.update(cx, |chrome, _| {
            chrome.content(left.into_any_element(), right.into_any_element())
        });
        self.window_chrome.clone().into_any_element()
    }
    fn render_navigation(&self, cx: &Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let mut navigation = div()
            .id("settings-navigation")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(8.));
        let mut has_extensions = false;
        for category in &self.registry.categories {
            if category.source.is_some() && !has_extensions {
                has_extensions = true;
                navigation = navigation.child(
                    div()
                        .mt(px(18.))
                        .mb(px(8.))
                        .px(px(12.))
                        .text_size(px(11.))
                        .text_color(palette.muted)
                        .child("插件"),
                );
            }
            let id = category.id.clone();
            let selected = self.query.is_empty() && id == self.category;
            navigation = navigation.child(
                button(format!("category-{id}"), &category.label, 34., palette)
                    .w_full()
                    .justify_start()
                    .px(px(12.))
                    .mb(px(2.))
                    .text_size(px(12.))
                    .text_color(if selected {
                        palette.accent
                    } else {
                        palette.text
                    })
                    .when(selected, |button| button.bg(palette.selection))
                    .when_some(category.source.clone(), |button, source| {
                        button.child(div().flex_1()).child(
                            div()
                                .text_size(px(10.))
                                .text_color(palette.muted)
                                .child(source),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.category = id.clone();
                        this.clear_search(window, cx);
                        this.message.clear();
                        cx.notify();
                    })),
            );
        }
        navigation.into_any_element()
    }
    fn render_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let layout = self.registry.layout.clone();
        let mut fields = div();
        let mut previous_category = String::new();
        let visible = self.visible_settings();
        for setting in visible
            .iter()
            .filter(|setting| setting.category != "core.keybindings")
        {
            if !self.query.is_empty() && setting.category != previous_category {
                let label = self
                    .registry
                    .categories
                    .iter()
                    .find(|category| category.id == setting.category)
                    .map(|category| category.label.clone())
                    .unwrap_or_default();
                fields = fields.child(
                    div()
                        .h(px(layout.section_header))
                        .flex()
                        .items_center()
                        .px(px(10.))
                        .bg(palette.subtle)
                        .border_b_1()
                        .border_color(palette.line)
                        .mb(px(28.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(label),
                );
                previous_category = setting.category.clone();
            }
            fields = fields.child(self.field(setting, window, cx));
        }
        let shortcuts: Vec<_> = visible
            .iter()
            .filter(|setting| setting.category == "core.keybindings")
            .cloned()
            .collect();
        if !shortcuts.is_empty() {
            fields = fields.child(self.shortcut_table(&shortcuts, window, cx));
        }
        if visible.is_empty() {
            fields = fields.child(
                div()
                    .py(px(20.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("没有匹配的设置"),
                    )
                    .child(
                        div()
                            .mt(px(5.))
                            .text_size(px(11.))
                            .text_color(palette.muted)
                            .child(if self.scope == Scope::Application {
                                "试试“字体”“换行”或“工作台”。"
                            } else {
                                "试试“编译器”“超时”或“比较”。"
                            }),
                    )
                    .child(
                        button("clear-search", "清除搜索", 28., palette)
                            .mt(px(14.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.clear_search(window, cx);
                                window.focus(&this.search.focus_handle(cx), cx);
                                cx.notify();
                            })),
                    ),
            );
        }
        fields.into_any_element()
    }
    fn shortcut_table(
        &mut self,
        settings: &[Setting],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = panel_style::palette(cx);
        let mut table = div()
            .child(
                div()
                    .mb(px(16.))
                    .text_size(px(12.))
                    .line_height(px(20.))
                    .text_color(palette.muted)
                    .child(
                        "使用 cmd / ctrl / alt / shift 组合；多个按键序列以空格分隔，留空停用。",
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(32.))
                    .bg(palette.subtle)
                    .border_t_1()
                    .border_b_1()
                    .border_color(palette.line)
                    .text_size(px(12.))
                    .text_color(palette.muted)
                    .child(div().flex_1().min_w_0().px(px(12.)).child("命令"))
                    .child(div().w(px(264.)).px(px(12.)).child("快捷键"))
                    .child(div().w(px(72.)).px(px(12.)).child("操作")),
            );
        for setting in settings {
            let input = self.ensure_input(setting, window, cx);
            let id = setting.id.clone();
            let default = setting.default_text();
            let error = self.errors.get(&id).cloned();
            table = table.child(
                div()
                    .flex()
                    .items_center()
                    .min_h(px(56.))
                    .border_b_1()
                    .border_color(palette.line)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px(px(12.))
                            .text_size(px(13.))
                            .child(setting.label.clone()),
                    )
                    .child(
                        div()
                            .w(px(264.))
                            .flex_shrink_0()
                            .px(px(12.))
                            .py(px(8.))
                            .child(
                                div()
                                    .h(px(36.))
                                    .border_1()
                                    .border_color(if error.is_some() {
                                        palette.error
                                    } else {
                                        palette.line
                                    })
                                    .track_focus(&input.focus_handle(cx))
                                    .tab_index(0_isize)
                                    .focus(|style| style.border_color(palette.accent))
                                    .child(input),
                            )
                            .when_some(error, |column, error| {
                                column.child(
                                    div()
                                        .mt(px(6.))
                                        .text_size(px(12.))
                                        .text_color(palette.error)
                                        .child(error),
                                )
                            }),
                    )
                    .child(
                        div().w(px(72.)).flex_shrink_0().px(px(12.)).child(
                            button(format!("reset-{id}"), "恢复", 28., palette)
                                .px(px(0.))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this.saving {
                                        return;
                                    }
                                    this.set_value(id.clone(), default.clone(), cx);
                                    if let Some(input) = this.inputs.get(&id) {
                                        input.field.update(cx, |input, cx| {
                                            input.set_text(default.clone(), window, cx)
                                        });
                                    }
                                })),
                        ),
                    ),
            );
        }
        table.into_any_element()
    }
    fn render_sidebar(&self, cx: &Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let layout = self.registry.layout.clone();
        div()
            .w(px(layout.sidebar))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(palette.subtle)
            .border_r_1()
            .border_color(palette.line)
            .child(
                div()
                    .mx(px(12.))
                    .mt(px(16.))
                    .mb(px(12.))
                    .h(px(30.))
                    .border_1()
                    .border_color(palette.line)
                    .track_focus(&self.search.focus_handle(cx))
                    .tab_index(0_isize)
                    .focus(|style| style.border_color(palette.accent))
                    .child(self.search.clone()),
            )
            .child(self.render_navigation(cx))
            .child(
                div()
                    .mx(px(20.))
                    .my(px(16.))
                    .text_size(px(11.))
                    .line_height(px(18.))
                    .text_color(palette.muted)
                    .child("插件启用后，其选项会出现在这里。"),
            )
            .into_any_element()
    }
    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let layout = self.registry.layout.clone();
        let category = self
            .registry
            .categories
            .iter()
            .find(|category| category.id == self.category)
            .cloned();
        let title = if self.query.is_empty() {
            category
                .as_ref()
                .map(|category| category.label.clone())
                .unwrap_or_default()
        } else {
            "搜索结果".into()
        };
        let description = if self.query.is_empty() {
            category
                .map(|category| category.description)
                .unwrap_or_default()
        } else {
            format!("包含“{}”的设置", self.query)
        };
        div()
            .id("settings-content")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .p(px(layout.content_padding))
            .when(self.message.starts_with("未"), |content| {
                content.child(
                    div()
                        .mb(px(16.))
                        .text_size(px(12.))
                        .line_height(px(20.))
                        .text_color(palette.error)
                        .child(self.message.clone()),
                )
            })
            .child(
                div()
                    .mb(px(32.))
                    .child(
                        div()
                            .h(px(26.))
                            .text_size(px(16.))
                            .line_height(px(26.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .when(
                        !self.query.is_empty() || self.category == "core.keybindings",
                        |header| {
                            header.child(
                                div()
                                    .mt(px(8.))
                                    .text_size(px(12.))
                                    .line_height(px(20.))
                                    .text_color(palette.muted)
                                    .child(description),
                            )
                        },
                    ),
            )
            .child(self.render_fields(window, cx))
            .into_any_element()
    }
    fn render_footer(&self, cx: &Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let layout = self.registry.layout.clone();
        let dirty = self.dirty();
        div()
            .h(px(layout.footer))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(16.))
            .border_t_1()
            .border_color(palette.line)
            .child(
                button("reset-settings", "恢复本页默认值", 28., palette).on_click(cx.listener(
                    |this, _, window, cx| {
                        if !this.saving {
                            this.reset(window, cx);
                        }
                    },
                )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_right()
                    .text_size(px(11.))
                    .text_color(palette.muted)
                    .child(if self.message.is_empty() {
                        if self.scope == Scope::Application {
                            "更改后点击应用保存".into()
                        } else {
                            "随文档保存运行配置".into()
                        }
                    } else {
                        self.message.clone()
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button("cancel-settings", "取消", 28., palette)
                            .on_click(|_, window, _| window.remove_window()),
                    )
                    .child(
                        button(
                            "apply-settings",
                            if self.saving {
                                "保存中…"
                            } else {
                                "应用"
                            },
                            28.,
                            palette,
                        )
                        .px(px(18.))
                        .border_color(palette.accent)
                        .text_color(palette.accent)
                        .when(!dirty || self.saving, |button| {
                            button.opacity(0.45).cursor_default().tab_stop(false)
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.apply(window, cx))),
                    ),
            )
            .into_any_element()
    }
}
impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = panel_style::palette(cx);
        div()
            .id("settings-window")
            .key_context("AltaedSettings")
            .track_focus(&self.focus)
            .tab_index(-1_isize)
            .tab_stop(false)
            .capture_key_down(|event, window, cx| {
                if event.keystroke.key == "tab"
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.alt
                {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                }
            })
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.paper)
            .text_color(palette.text)
            .text_size(px(12.))
            .line_height(px(18.))
            .on_action(cx.listener(|this, _: &CloseSettings, window, cx| {
                if !this.query.is_empty() {
                    this.clear_search(window, cx);
                    cx.notify();
                } else {
                    window.remove_window();
                }
            }))
            .on_action(|_: &CloseDocument, window, cx| {
                cx.stop_propagation();
                window.remove_window();
            })
            .on_action(cx.listener(|this, _: &SaveDocument, window, cx| {
                cx.stop_propagation();
                this.apply(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SearchSettings, window, cx| {
                window.focus(&this.search.focus_handle(cx), cx)
            }))
            .child(self.render_header(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(self.render_content(window, cx)),
            )
            .child(self.render_footer(cx))
    }
}

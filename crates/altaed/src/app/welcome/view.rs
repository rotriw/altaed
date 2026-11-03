use super::*;
use crate::panel_style::{self, form_button as button, primary_form_button};
use gpui::{
    Animation, AnimationExt, AnyElement, FontWeight, IntoElement, Render, div, prelude::*, svg,
};
use std::time::Duration;

impl Welcome {
    fn background_banner(&self, cx: &Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let banner = div()
            .absolute()
            .bottom(px(self.layout.banner_bottom))
            .left_0()
            .w_full()
            .h(px(176.))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .text_size(px(self.layout.banner_font_size))
            .line_height(px(176.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(palette.line)
            .children(
                "ALTAED"
                    .chars()
                    .map(|character| div().child(character.to_string())),
            );
        if self.animate {
            let min = self.layout.banner_opacity_min;
            let max = self.layout.banner_opacity_max;
            banner
                .with_animation(
                    "welcome-banner",
                    Animation::new(Duration::from_secs(self.layout.breath_duration_seconds))
                        .repeat(),
                    move |banner, progress| {
                        banner.opacity(
                            min + (max - min) * (1. - (std::f32::consts::TAU * progress).cos())
                                / 2.,
                        )
                    },
                )
                .into_any_element()
        } else {
            banner
                .opacity((self.layout.banner_opacity_min + self.layout.banner_opacity_max) / 2.)
                .into_any_element()
        }
    }
    fn wordmark(&self, cx: &Context<Self>) -> AnyElement {
        let metrics = &self.layout.wordmark;
        // GPUI's monochrome SVG cache retains the Gaussian shadow's alpha;
        // tinting it with the text color also keeps dark appearances legible.
        div()
            .relative()
            .w(px(metrics.width))
            .h(px(metrics.height))
            .flex_shrink_0()
            .child(
                svg()
                    .path(crate::app_assets::WORDMARK)
                    .absolute()
                    .left(px(-metrics.padding))
                    .top(px(-metrics.padding))
                    .w(px(metrics.width + 2. * metrics.padding))
                    .h(px(metrics.height + 2. * metrics.padding))
                    .text_color(panel_style::palette(cx).accent),
            )
            .into_any_element()
    }
    fn hero(&self, cx: &Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .p(px(40.))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(28.))
                            .line_height(px(38.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("欢迎使用")
                            .child(self.wordmark(cx)),
                    )
                    .child(
                        div()
                            .mt(px(8.))
                            .text_size(px(13.))
                            .line_height(px(22.))
                            .text_color(palette.muted)
                            .child("你的本地竞赛编程 IDE。"),
                    ),
            )
            .into_any_element()
    }
    fn environment(&self, cx: &Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        div()
            .child(
                div()
                    .text_size(px(20.))
                    .line_height(px(28.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("配置编译环境"),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_color(palette.muted)
                    .child("检测本地 C++ 编译器，也可以稍后配置。"),
            )
            .child(
                div()
                    .mt(px(20.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("C++ 编译器"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .mt(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(self.layout.control_height))
                                    .border_1()
                                    .border_color(palette.line)
                                    .track_focus(&self.compiler.focus_handle(cx))
                                    .tab_index(0_isize)
                                    .focus(|style| style.border_color(palette.accent))
                                    .child(self.compiler.clone()),
                            )
                            .child(
                                button("choose-compiler", "选择文件…", 32., palette)
                                    .px(px(12.))
                                    .border_color(palette.line)
                                    .when(self.busy, |control| {
                                        control.opacity(0.45).cursor_default().tab_stop(false)
                                    })
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.choose(window, cx)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .mt(px(8.))
                            .child(
                                button("detect-compiler", "自动查找", 28., palette)
                                    .when(self.busy, |control| {
                                        control.opacity(0.45).cursor_default().tab_stop(false)
                                    })
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.detect(window, cx)),
                                    ),
                            )
                            .child(
                                button("verify-compiler", "验证环境", 28., palette)
                                    .border_color(palette.line)
                                    .text_color(palette.text)
                                    .px(px(16.))
                                    .when(self.busy, |control| {
                                        control.opacity(0.45).cursor_default().tab_stop(false)
                                    })
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.verify(window, cx)),
                                    ),
                            ),
                    )
                    .when_some(self.verified.as_ref(), |page, compiler| {
                        let mut choices = div().flex().flex_wrap().gap(px(4.));
                        for &standard in &compiler.standards {
                            let selected = self.cpp_standard == standard;
                            choices = choices.child(
                                button(
                                    format!("welcome-standard-{}", standard.value()),
                                    standard.label(),
                                    26.,
                                    palette,
                                )
                                .px(px(8.))
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
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if !this.busy {
                                            this.cpp_standard = standard;
                                            cx.notify();
                                        }
                                    },
                                )),
                            );
                        }
                        page.child(
                            div()
                                .mt(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(div().flex_shrink_0().child("C++ 标准"))
                                .child(choices),
                        )
                    })
                    .child(
                        div()
                            .min_h(px(40.))
                            .pt(px(12.))
                            .flex()
                            .items_start()
                            .gap(px(8.))
                            .text_color(if self.error {
                                palette.error
                            } else if self.verified.is_some() {
                                palette.success
                            } else {
                                palette.muted
                            })
                            .when(self.verified.is_some(), |row| {
                                row.child(
                                    svg()
                                        .path(crate::app_assets::STATUS_SUCCESS)
                                        .size(px(18.))
                                        .mt(px(1.))
                                        .flex_shrink_0()
                                        .text_color(palette.success),
                                )
                            })
                            .child(self.message.clone()),
                    ),
            )
            .into_any_element()
    }
    fn shortcuts(&self, cx: &Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let mut table = div().mt(px(16.)).child(
            div()
                .flex()
                .items_center()
                .h(px(28.))
                .bg(palette.subtle)
                .border_t_1()
                .border_b_1()
                .border_color(palette.line)
                .text_color(palette.muted)
                .child(div().w(px(148.)).flex_shrink_0().px(px(12.)).child("命令"))
                .child(
                    div()
                        .w(px(240.))
                        .flex_shrink_0()
                        .px(px(12.))
                        .child("快捷键"),
                )
                .child(div().flex_1().min_w_0().px(px(12.)).child("行为")),
        );
        for (input, (_, behavior)) in self.shortcuts.iter().zip(EXECUTION_COMMANDS) {
            let invalid = self
                .shortcut_errors
                .contains_key(&input.command.setting_id());
            table = table.child(
                div()
                    .flex()
                    .items_center()
                    .min_h(px(self.layout.table_row))
                    .border_b_1()
                    .border_color(palette.line)
                    .child(
                        div()
                            .w(px(148.))
                            .flex_shrink_0()
                            .px(px(12.))
                            .child(input.command.label.clone()),
                    )
                    .child(
                        div().w(px(240.)).flex_shrink_0().px(px(12.)).child(
                            div()
                                .h(px(32.))
                                .border_1()
                                .border_color(if invalid { palette.error } else { palette.line })
                                .track_focus(&input.field.focus_handle(cx))
                                .tab_index(0_isize)
                                .focus(|style| style.border_color(palette.accent))
                                .child(input.field.clone()),
                        ),
                    )
                    .child(div().flex_1().min_w_0().px(px(12.)).child(behavior)),
            );
        }
        div()
            .child(
                div()
                    .text_size(px(20.))
                    .line_height(px(28.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("确认执行快捷键"),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_color(palette.muted)
                    .child("随时可在软件设置中修改。"),
            )
            .child(table)
            .child(
                div()
                    .mt(px(8.))
                    .text_color(palette.muted)
                    .child("使用 cmd / ctrl / alt / shift 组合，留空可停用。"),
            )
            .child(
                div()
                    .mt(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(div().text_color(palette.muted).child("配置保存在本机。"))
                    .child(
                        button("show-config", "显示 config.json", 28., palette)
                            .text_color(palette.accent)
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.reveal_path(
                                    &this.startup.directory.join(configuration::FILE_NAME),
                                )
                            })),
                    ),
            )
            .when(!self.shortcut_message.is_empty(), |page| {
                page.child(
                    div()
                        .mt(px(8.))
                        .text_color(palette.error)
                        .child(self.shortcut_message.clone()),
                )
            })
            .into_any_element()
    }
    fn footer(&self, cx: &Context<Self>) -> AnyElement {
        let palette = panel_style::palette(cx);
        let enabled = !self.busy && (self.page != Page::Environment || self.verified.is_some());
        let label = match self.page {
            Page::Welcome => "开始配置",
            Page::Environment => "继续",
            Page::Shortcuts => {
                if self.saving {
                    "保存中…"
                } else {
                    "完成并进入 IDE"
                }
            }
        };
        div()
            .absolute()
            .bottom_0()
            .w_full()
            .h(px(self.layout.footer))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(24.))
            .child(
                button(
                    "welcome-back",
                    if self.page == Page::Welcome {
                        "退出"
                    } else {
                        "上一步"
                    },
                    28.,
                    palette,
                )
                .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .text_right()
                    .text_size(px(11.))
                    .text_color(palette.muted)
                    .child(if self.page == Page::Environment {
                        if self.verified.is_some() {
                            "环境已通过"
                        } else if self.busy {
                            "正在检测…"
                        } else {
                            "可稍后在软件设置中配置"
                        }
                    } else {
                        ""
                    }),
            )
            .when(
                self.page == Page::Environment && self.verified.is_none(),
                |row| {
                    row.child(
                        button("skip-environment", "稍后配置", 28., palette).on_click(
                            cx.listener(|this, _, window, cx| this.skip_environment(window, cx)),
                        ),
                    )
                },
            )
            .child(
                primary_form_button("welcome-next", label, 32., palette)
                    .px(px(20.))
                    .when(!enabled, |control| {
                        control.opacity(0.45).cursor_default().tab_stop(false)
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.next(window, cx))),
            )
            .into_any_element()
    }
}
impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = panel_style::palette(cx);
        let content = match self.page {
            Page::Welcome => self.hero(cx),
            page => div()
                .id("welcome-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(self.layout.padding))
                .pt(px(24.))
                .pb(px(8.))
                .mb(px(self.layout.footer))
                .child(if page == Page::Environment {
                    self.environment(cx)
                } else {
                    self.shortcuts(cx)
                })
                .into_any_element(),
        };
        div()
            .id("welcome")
            .track_focus(&self.focus)
            .tab_index(-1_isize)
            .tab_stop(false)
            .size_full()
            .relative()
            .rounded(px(12.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(palette.paper)
            .text_color(palette.text)
            .font_family(".ZedSans")
            .text_size(px(12.))
            .line_height(px(20.))
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
            .on_action(|_: &super::super::CloseDocument, _, cx| cx.quit())
            .when(self.page == Page::Welcome, |root| {
                root.child(self.background_banner(cx))
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(32.))
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_move()
                    }),
            )
            .child(content)
            .child(self.footer(cx))
    }
}

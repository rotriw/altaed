//! Native workbench composition. Document storage and editor internals stay outside this module.
mod document_tabs;
mod editor_pane;
pub(crate) mod sidebar;
use super::*;
use crate::workbench_layout::{ContextTab, Panel, RESIZE_HANDLE, RESIZE_HIT_AREA, STATUS_HEIGHT};
use gpui::{AnyElement, DragMoveEvent, MouseButton, Pixels, div, px};
use settings::Settings;
use theme::ThemeSettings;
use ui::{Tooltip, prelude::*};

#[derive(Clone)]
struct PanelDrag(Panel);
impl Render for PanelDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Ide {
    fn messages_height(&self, id: DocumentId) -> f32 {
        self.layout.messages_height(
            self.runs
                .get(&id)
                .is_some_and(|run| run.running || !run.brief.is_empty()),
        )
    }
    fn document_label(&self, index: usize) -> String {
        self.session.documents[index].display_name()
    }

    pub(super) fn toggle_panel(
        &mut self,
        panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layout.toggle(panel);
        window.focus(&self.views[self.session.active].editor.focus_handle(cx), cx);
        cx.notify();
    }

    fn resize_handle(&self, panel: Panel, cx: &Context<Self>) -> AnyElement {
        let colors = crate::panel_style::palette(cx);
        let horizontal = matches!(panel, Panel::Output | Panel::Messages);
        let hit_area = div()
            .id(match panel {
                Panel::Documents => "documents-resize",
                Panel::Context => "context-resize",
                Panel::Output => "output-resize",
                Panel::Messages => "messages-resize",
            })
            .debug_selector(move || {
                match panel {
                    Panel::Documents => "documents-resize",
                    Panel::Context => "context-resize",
                    Panel::Output => "output-resize",
                    Panel::Messages => "messages-resize",
                }
                .into()
            })
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .absolute()
            .map(|element| {
                if horizontal {
                    element
                        .top(px(-(RESIZE_HIT_AREA - RESIZE_HANDLE) / 2.))
                        .left_0()
                        .h(px(RESIZE_HIT_AREA))
                        .w_full()
                        .cursor_row_resize()
                } else {
                    element
                        .left(px(-(RESIZE_HIT_AREA - RESIZE_HANDLE) / 2.))
                        .top_0()
                        .w(px(RESIZE_HIT_AREA))
                        .h_full()
                        .cursor_col_resize()
                }
            })
            .focusable()
            .tab_index(0_isize)
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    let delta = match event.keystroke.key.as_str() {
                        "left" if !horizontal => -16.0,
                        "right" if !horizontal => 16.0,
                        "up" if horizontal => 16.0,
                        "down" if horizontal => -16.0,
                        _ => return,
                    };
                    let delta = if matches!(panel, Panel::Context | Panel::Messages) {
                        -delta
                    } else {
                        delta
                    };
                    let viewport = window.viewport_size();
                    let (left, right, bottom) = this.layout.sizes(
                        viewport.width.into(),
                        (viewport.height - super::window_chrome::height(window)).into(),
                    );
                    let size = match panel {
                        Panel::Documents => left,
                        Panel::Context => right,
                        Panel::Output => bottom,
                        Panel::Messages => {
                            this.messages_height(this.session.documents[this.session.active].id)
                        }
                    };
                    this.layout.set_size(panel, size + delta);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_drag(PanelDrag(panel), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseUpEvent, _, cx| {
                    if event.click_count == 2 {
                        this.layout.reset_size(panel);
                        cx.notify();
                    }
                }),
            )
            .tooltip(Tooltip::text("拖动调整大小；双击恢复；聚焦后可用方向键"));
        // The hit target overlaps adjacent panes. Only the one-pixel separator
        // participates in layout, so panel headers meet the boundary directly.
        div()
            .id(match panel {
                Panel::Documents => "documents-separator",
                Panel::Context => "context-separator",
                Panel::Output => "output-separator",
                Panel::Messages => "messages-separator",
            })
            .debug_selector(move || match panel {
                Panel::Documents => "documents-separator".into(),
                Panel::Context => "context-separator".into(),
                Panel::Output => "output-separator".into(),
                Panel::Messages => "messages-separator".into(),
            })
            .relative()
            .flex_shrink_0()
            .bg(colors.line)
            .map(|line| {
                if horizontal {
                    line.h(px(RESIZE_HANDLE)).w_full()
                } else {
                    line.w(px(RESIZE_HANDLE)).h_full()
                }
            })
            .child(hit_area)
            .into_any_element()
    }

    fn render_context(
        &self,
        document: &CodeDocument,
        width: Pixels,
        cx: &Context<Self>,
    ) -> AnyElement {
        let c = crate::panel_style::palette(cx);
        let heading = |height| {
            div()
                .h(px(height))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_between()
                .pl(px(12.))
                .pr(px(8.))
                .bg(c.subtle)
                .border_b_1()
                .border_color(c.line)
        };
        let title = |text: &'static str| {
            div()
                .text_size(px(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(text)
        };
        let mut panel = div()
            .w(width)
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(c.paper)
            .text_color(c.text)
            .text_size(px(12.));
        if self.layout.context_tab == ContextTab::Problem {
            return panel
                .child(heading(30.).child(title("题目信息")).child(
                    crate::panel_style::button("return-tests", "返回测试", cx).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.layout.context_tab = ContextTab::Tests;
                            cx.notify();
                        }),
                    ),
                ))
                .child(
                    div()
                        .id("problem-details")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p(px(12.))
                        .child(
                            document.metadata.problem.as_ref().map(|problem| problem.title.clone())
                                .or_else(|| document.metadata.problem_title.clone())
                                .unwrap_or_else(|| "未绑定题目".into()),
                        )
                        .when_some(document.metadata.problem.as_ref().map(|problem| problem.url.clone())
                            .or_else(|| document.metadata.problem_url.clone()), |d, url| {
                            d.child(div().mt(px(8.)).text_color(c.muted).child(url))
                        })
                        .when_some(document.metadata.problem.as_ref(), |d, problem| {
                            let input = match &problem.input {
                                altaed_core::problem::ProblemInput::Stdin => "标准输入".to_owned(),
                                altaed_core::problem::ProblemInput::File { file_name } => format!("输入文件：{file_name}"),
                                altaed_core::problem::ProblemInput::Regex { pattern } => format!("输入文件匹配：{pattern}"),
                            };
                            let output = match &problem.output {
                                altaed_core::problem::ProblemOutput::Stdout => "标准输出".to_owned(),
                                altaed_core::problem::ProblemOutput::File { file_name } => format!("输出文件：{file_name}"),
                            };
                            d.when(!problem.group.is_empty(), |d| d.child(div().mt(px(12.)).child(problem.group.clone())))
                                .when_some(problem.time_limit_ms, |d, limit| d.child(div().mt(px(8.)).child(format!("题目时间限制：{limit} ms"))))
                                .when_some(problem.memory_limit_mb, |d, limit| d.child(div().mt(px(8.)).child(format!("题目内存限制：{limit} MB"))))
                                .child(div().mt(px(8.)).child(input))
                                .child(div().mt(px(8.)).child(output))
                                .child(div().mt(px(8.)).child(format!("原始样例：{} 组", problem.samples.len())))
                                .when(!problem.supports_sample_runner(), |d| d.child(div().mt(px(12.)).text_color(c.muted)
                                    .child("交互或文件输入输出暂不支持自动样例评测；信息已保留，可编译后在终端运行。")))
                        }),
                )
                .into_any_element();
        }
        if self.layout.messages_visible {
            panel = panel
                .child(
                    div()
                        .h(px(self.messages_height(document.id)))
                        .max_h(gpui::relative(0.35))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .child(heading(30.).child(title("运行信息")).child(
                            crate::panel_style::button("clear-messages", "清空", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    let id = this.session.documents[this.session.active].id;
                                    if let Some(run) = this.runs.get_mut(&id) {
                                        run.brief.clear();
                                    }
                                    cx.notify();
                                }),
                            ),
                        ))
                        .child(
                            div()
                                .id("test-messages")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .px(px(12.))
                                .py(px(10.))
                                .font_family(crate::platform::code_font())
                                .text_size(px(11.))
                                .line_height(px(19.))
                                .text_color(c.muted)
                                .child(
                                    self.runs
                                        .get(&document.id)
                                        .map(|r| {
                                            format!(
                                                "{}{}",
                                                if r.stale && !r.brief.is_empty() {
                                                    "内容已修改，以下是旧版本结果。\n"
                                                } else {
                                                    ""
                                                },
                                                r.brief
                                            )
                                        })
                                        .unwrap_or_else(|| {
                                            "尚未运行。编辑用例后可运行测试。".into()
                                        }),
                                ),
                        ),
                )
                .child(self.resize_handle(Panel::Messages, cx));
        }
        panel
            .child(
                heading(36.)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(title("测试数据"))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(c.muted)
                                    .child(document.metadata.tests.len().to_string()),
                            ),
                    )
                    .child(
                        crate::panel_style::button("add-inline-test", "添加用例", cx)
                            .icon(IconName::Plus)
                            .icon_position(IconPosition::Start)
                            .icon_size(IconSize::Custom(gpui::rems(
                                px(14.) / ThemeSettings::get_global(cx).ui_font_size(cx),
                            )))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_test(&AddTest, window, cx)
                            })),
                    ),
            )
            .child(self.render_checker_controls(document, cx))
            .when(document.metadata.tests.is_empty(), |d| {
                d.child(
                    div()
                        .p(px(12.))
                        .text_color(c.muted)
                        .child(div().text_size(px(12.)).child("还没有测试用例"))
                        .child(
                            div()
                                .mt(px(6.))
                                .text_size(px(11.))
                                .child("添加输入和预期输出，即可测试当前代码。"),
                        )
                        .child(
                            crate::panel_style::button("add-first-test", "添加第一个用例", cx)
                                .color(Color::Custom(c.accent))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.add_test(&AddTest, window, cx)
                                })),
                        ),
                )
            })
            .child(
                gpui::uniform_list(
                    "test-cases",
                    document.metadata.tests.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, window, cx| {
                        let id = this.session.documents[this.session.active].id;
                        this.ensure_test_editors(id, range.clone(), window, cx);
                        let doc = &this.session.documents[this.session.active];
                        range
                            .map(|index| {
                                this.render_test_case(doc, index, &doc.metadata.tests[index], cx)
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }

    fn render_output(&self, height: Pixels, cx: &Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        div()
            .h(height)
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(colors.editor_background)
            .overflow_hidden()
            .child(
                div()
                    .h(px(30.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .border_b_1()
                    .border_color(colors.border)
                    .text_xs()
                    .child("构建输出")
                    .child(
                        IconButton::new("hide-output", IconName::Close)
                            .style(ButtonStyle::Transparent)
                            .size(ButtonSize::Compact)
                            .tab_index(0_isize)
                            .tooltip(Tooltip::text("收起输出"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_panel(Panel::Output, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("output-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_3()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .font_family(ThemeSettings::get_global(cx).buffer_font.family.clone())
                    .child({
                        let doc = &self.session.documents[self.session.active];
                        match self.runs.get(&doc.id) {
                            None => "尚未执行。使用“运行”菜单编译、测试或打开新终端。".to_owned(),
                            Some(run) => {
                                let changed = run.stale;
                                format!(
                                    "{}{}",
                                    if changed {
                                        "代码或配置已修改；以下结果对应执行时的旧版本。\n"
                                    } else {
                                        ""
                                    },
                                    run.message
                                )
                            }
                        }
                    }),
            )
            .into_any_element()
    }
}

impl Render for Ide {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_companion(window, cx);
        self.sync_checker_controls(window, cx);
        if self.preferences_pending {
            self.preferences_pending = false;
            for view in &self.views {
                view.apply_preferences(&self.preferences.editor, cx);
            }
        }
        let snapshot = &self.session;
        let colors = cx.theme().colors();
        let viewport = window.viewport_size();
        let (left, right, bottom) = self.layout.sizes(
            viewport.width.into(),
            (viewport.height - super::window_chrome::height(window)).into(),
        );
        let current = &snapshot.documents[self.session.active];
        let visibility = (
            current.id,
            self.session.active,
            viewport.width - px(left + right + sidebar::ACTIVITY_BAR_WIDTH),
        );
        if self.tab_visibility != Some(visibility) {
            self.tab_visibility = Some(visibility);
            self.tabs_scroll.scroll_to_item(self.session.active);
            // Scroll against the newly laid-out width, rather than the
            // previous frame's viewport when a window or sidebar resizes.
            let view = cx.weak_entity();
            window.on_next_frame(move |_, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.tabs_scroll.scroll_to_item(this.session.active);
                    cx.notify();
                });
            });
        }
        window.set_window_title(&format!(
            "{} — altaed",
            self.document_label(self.session.active)
        ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.editor_background)
            .text_color(colors.text)
            .text_sm()
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && cx.stop_active_drag(window) {
                    this.tab_auto_scroll = None;
                    this.tabs_scroll.scroll_to_item(this.session.active);
                    window.focus(&this.views[this.session.active].editor.focus_handle(cx), cx);
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(Self::close_document))
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(|this, _: &ShowTests, _, cx| {
                this.layout.context_visible = true;
                this.layout.context_tab = ContextTab::Tests;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowProblem, _, cx| {
                this.layout.context_visible = true;
                this.layout.context_tab = ContextTab::Problem;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CompileDocument, window, cx| {
                this.start_run(crate::runner::RunMode::Compile, window, cx)
            }))
            .on_action(cx.listener(|this, _: &RunTests, window, cx| {
                this.start_run(crate::runner::RunMode::Tests, window, cx)
            }))
            .on_action(cx.listener(Ide::run_compiled))
            .on_action(cx.listener(|this, _: &CompileAndRun, window, cx| {
                this.start_run(crate::runner::RunMode::CompileAndRun, window, cx)
            }))
            .on_action(cx.listener(Self::cancel_run))
            .on_action(cx.listener(Self::edit_configuration))
            .on_action(cx.listener(Self::go_to_definition))
            .on_action(cx.listener(Self::restart_language_server))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::reload_configuration))
            .on_action(cx.listener(Self::reveal_configuration))
            .on_action(cx.listener(Self::add_test))
            .on_action(cx.listener(|this, _: &ToggleDocuments, window, cx| {
                this.toggle_panel(Panel::Documents, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleContext, window, cx| {
                this.toggle_panel(Panel::Context, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleOutput, window, cx| {
                this.toggle_panel(Panel::Output, window, cx)
            }))
            // A quick gesture can start a drag on its last move event. Commit the
            // release position too, rather than requiring another move to resize.
            .on_drop(cx.listener(|this, drag: &PanelDrag, window, cx| {
                let pointer = window.mouse_position();
                let viewport = window.viewport_size();
                let extent = match drag.0 {
                    Panel::Documents => pointer.x - px(sidebar::ACTIVITY_BAR_WIDTH),
                    Panel::Context => viewport.width - pointer.x,
                    Panel::Output => viewport.height - px(STATUS_HEIGHT) - pointer.y,
                    Panel::Messages => pointer.y - super::window_chrome::height(window),
                };
                this.layout.set_size(drag.0, extent.into());
                cx.notify();
            }))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<PanelDrag>, window, cx| {
                    let panel = event.drag(cx).0;
                    let extent = match panel {
                        Panel::Documents => {
                            event.event.position.x
                                - event.bounds.left()
                                - px(sidebar::ACTIVITY_BAR_WIDTH)
                        }
                        Panel::Context => event.bounds.right() - event.event.position.x,
                        Panel::Output => {
                            event.bounds.bottom() - px(STATUS_HEIGHT) - event.event.position.y
                        }
                        Panel::Messages => {
                            event.event.position.y
                                - event.bounds.top()
                                - super::window_chrome::height(window)
                        }
                    };
                    this.layout.set_size(panel, extent.into());
                    cx.notify();
                }),
            )
            .child(self.render_title_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_activity_bar(cx))
                    .when(self.layout.documents_visible, |element| {
                        element
                            .child(self.render_resources(snapshot, px(left), cx))
                            .child(self.resize_handle(Panel::Documents, cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_h_0()
                                    .child(self.render_editor(snapshot, cx))
                                    .when(self.layout.context_visible, |element| {
                                        element
                                            .child(self.resize_handle(Panel::Context, cx))
                                            .child(self.render_context(current, px(right), cx))
                                    }),
                            )
                            .when(self.layout.output_visible, |element| {
                                element
                                    .child(self.resize_handle(Panel::Output, cx))
                                    .child(self.render_output(px(bottom), cx))
                            }),
                    ),
            )
            .child(self.render_status_bar(current, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, point, size};

    #[gpui::test]
    fn panel_handles_resize_through_real_mouse_events(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings = settings::SettingsStore::test(cx);
            cx.set_global(settings);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
        });
        let directory =
            std::env::temp_dir().join(format!("altaed-layout-{}", uuid::Uuid::new_v4()));
        let language = Arc::new(Language::new(
            toml::from_str(include_str!("../../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let (ide, cx) = cx.add_window_view(|window, cx| {
            let mut session = Session::default();
            session.ensure_document();
            Ide::new(session, directory, language, window, cx)
        });
        cx.simulate_resize(size(px(1250.), px(800.)));
        for (panel, selector, delta) in [
            (Panel::Context, "context-resize", point(px(-80.), px(0.))),
            (Panel::Documents, "documents-resize", point(px(60.), px(0.))),
            (Panel::Output, "output-resize", point(px(0.), px(-60.))),
            (Panel::Messages, "messages-resize", point(px(0.), px(40.))),
        ] {
            ide.update(cx, |ide, cx| {
                ide.layout.documents_visible = true;
                ide.layout.output_visible = true;
                cx.notify();
            });
            cx.run_until_parked();
            let separator_selector = match panel {
                Panel::Documents => "documents-separator",
                Panel::Context => "context-separator",
                Panel::Output => "output-separator",
                Panel::Messages => "messages-separator",
            };
            let separator = cx.debug_bounds(separator_selector).unwrap();
            let target = cx.debug_bounds(selector).unwrap();
            if matches!(panel, Panel::Messages | Panel::Output) {
                assert_eq!(separator.size.height, px(1.));
                assert_eq!(target.size.height, px(5.));
            } else {
                assert_eq!(separator.size.width, px(1.));
                assert_eq!(target.size.width, px(5.));
            }
            let before = ide.read_with(cx, |ide, _| {
                if matches!(panel, Panel::Messages) {
                    ide.messages_height(ide.session.documents[ide.session.active].id)
                } else {
                    ide.layout.requested_size(panel)
                }
            });
            let start = cx
                .debug_bounds(selector)
                .expect("visible resize handle")
                .center();
            cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_move(start + delta, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(start + delta, MouseButton::Left, Modifiers::default());
            cx.run_until_parked();
            let after = ide.read_with(cx, |ide, _| ide.layout.requested_size(panel));
            assert!(
                after > before + 30.,
                "{panel:?} should grow after dragging: {before} -> {after}"
            );
            ide.update(cx, |ide, cx| {
                ide.layout.toggle(panel);
                cx.notify();
            });
            cx.run_until_parked();
            ide.update(cx, |ide, cx| {
                ide.layout.toggle(panel);
                cx.notify();
            });
            cx.run_until_parked();
            assert_eq!(
                ide.read_with(cx, |ide, _| ide.layout.requested_size(panel)),
                after
            );
            let reset = cx.debug_bounds(selector).unwrap().center();
            cx.simulate_event(gpui::MouseDownEvent {
                position: reset,
                button: MouseButton::Left,
                click_count: 2,
                modifiers: Modifiers::default(),
                first_mouse: false,
            });
            cx.simulate_event(gpui::MouseUpEvent {
                position: reset,
                button: MouseButton::Left,
                click_count: 2,
                modifiers: Modifiers::default(),
            });
            cx.run_until_parked();
            assert_eq!(
                ide.read_with(cx, |ide, _| ide.layout.requested_size(panel)),
                WorkbenchLayout::default().requested_size(panel)
            );
        }
    }
    #[gpui::test]
    fn active_tabs_scroll_into_view_and_closing_last_tab_does_not_reselect_removed_index(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
        });
        let directory = std::env::temp_dir().join(format!("altaed-tabs-{}", uuid::Uuid::new_v4()));
        let language = Arc::new(Language::new(
            toml::from_str(include_str!("../../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let (ide, cx) = cx.add_window_view(|window, cx| {
            let documents = (0..18)
                .map(|index| {
                    let mut document = CodeDocument::untitled();
                    document.path = Some(
                        std::env::temp_dir()
                            .join(format!("long_solution_name_{index}_for_tab_overflow.cpp")),
                    );
                    document
                })
                .collect();
            Ide::new(
                Session {
                    documents,
                    active: 0,
                },
                directory,
                language,
                window,
                cx,
            )
        });
        cx.simulate_resize(size(px(900.), px(500.)));
        cx.run_until_parked();
        cx.update(|window, cx| ide.update(cx, |ide, cx| ide.activate_document(17, window, cx)));
        cx.run_until_parked();
        let active = cx.debug_bounds("TAB-document-tab-17").unwrap();
        let viewport = ide.read_with(cx, |ide, _| ide.tabs_scroll.bounds());
        assert!(
            active.left() >= viewport.left() && active.right() <= viewport.right(),
            "selected tab must be visible with many files: {active:?}, viewport {viewport:?}"
        );
        // Native close slot ends at the tab's right edge. Clicking it bubbles
        // through the Tab unless the close handler consumes the event.
        cx.simulate_click(
            point(active.right() - px(12.), active.center().y),
            Modifiers::default(),
        );
        cx.run_until_parked();
        ide.read_with(cx, |ide, _| {
            assert_eq!(ide.session.documents.len(), 17);
            assert_eq!(ide.session.active, 16);
        });
        cx.update(|window, cx| ide.update(cx, |ide, cx| ide.activate_document(0, window, cx)));
        cx.run_until_parked();
        let first = cx.debug_bounds("TAB-document-tab-0").unwrap();
        let viewport = ide.read_with(cx, |ide, _| ide.tabs_scroll.bounds());
        assert!(first.left() >= viewport.left() && first.right() <= viewport.right());
    }
}

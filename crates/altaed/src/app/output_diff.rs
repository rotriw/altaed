//! Read-only comparison of the immutable expected/actual values of one run.
use super::*;
use altaed_core::output_diff::{DiffLine, OutputDiff};
use gpui::{
    AnyElement, AppContext, Bounds, SharedString, UniformListScrollHandle, WindowBounds,
    WindowOptions, div, px, size,
};
use ui::prelude::*;

struct DisplayLine {
    number: String,
    plain: [SharedString; 3],
    whitespace: [SharedString; 3],
}
impl DisplayLine {
    fn new(line: DiffLine) -> Self {
        let parts = [
            &line.text[..line.changed.start],
            &line.text[line.changed.clone()],
            &line.text[line.changed.end..],
        ];
        let plain =
            parts.map(|text| SharedString::from(text.trim_end_matches(['\r', '\n']).to_owned()));
        let mut whitespace = parts.map(|text| {
            SharedString::from(
                text.replace(' ', "·")
                    .replace('\t', "⇥")
                    .replace('\r', "␍")
                    .replace('\n', "↵"),
            )
        });
        if !line.text.ends_with('\n') {
            whitespace[2] = format!("{} ⟪EOF⟫", whitespace[2]).into();
        }
        Self {
            number: line.number.to_string(),
            plain,
            whitespace,
        }
    }
}
struct Row {
    expected: Option<DisplayLine>,
    actual: Option<DisplayLine>,
    changed: bool,
}
struct Content {
    rows: Vec<Row>,
    changes: Vec<usize>,
    truncated: bool,
    invalid_utf8: bool,
    plain_width: gpui::Pixels,
    whitespace_width: gpui::Pixels,
}
struct DiffWindow {
    content: Option<Content>,
    scroll: UniformListScrollHandle,
    whitespace: bool,
    current: usize,
    description: String,
    _loading: Option<gpui::Task<()>>,
    horizontal: gpui::Pixels,
}
impl Ide {
    pub(super) fn show_output_diff(&mut self, index: usize, cx: &mut Context<Self>) {
        let id = self.session.documents[self.session.active].id;
        let Some(run) = self.runs.get(&id) else {
            return;
        };
        let Some(report) = &run.report else {
            return;
        };
        let Some(actual) = report.cases.iter().find(|case| case.index == index) else {
            return;
        };
        let Some(expected) = report
            .test_snapshot
            .iter()
            .find(|test| test.index == index)
            .and_then(|test| test.expected_output.clone())
        else {
            return;
        };
        let actual_text = actual.output.stdout.clone();
        let description = format!(
            "{} · {}{}",
            actual.verdict.label(),
            report.snapshot.metadata.settings.execution.checker_label(),
            if run.stale {
                " · 旧版本运行快照"
            } else {
                " · 本次运行快照"
            }
        );
        let bounds = Bounds::centered(None, size(px(1000.), px(640.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(660.), px(360.))),
                ..Default::default()
            },
            move |window, cx| {
                window.set_window_title(&format!("用例 {} · 输出差异 — altaed", index + 1));
                theme::setup_ui_font(window, cx);
                cx.new(|cx: &mut Context<DiffWindow>| {
                    let mut view = DiffWindow {
                        content: None,
                        scroll: UniformListScrollHandle::new(),
                        whitespace: true,
                        current: 0,
                        description,
                        _loading: None,
                        horizontal: px(0.),
                    };
                    let text_system = window.text_system().clone();
                    view._loading = Some(cx.spawn(async move |this, cx| {
                        let content = cx
                            .background_spawn(async move {
                                let diff = OutputDiff::between(&expected, &actual_text);
                                let rows: Vec<Row> = diff
                                    .rows
                                    .into_iter()
                                    .map(|row| Row {
                                        expected: row.expected.map(DisplayLine::new),
                                        actual: row.actual.map(DisplayLine::new),
                                        changed: row.changed,
                                    })
                                    .collect();
                                let measure = |parts: &[SharedString; 3]| {
                                    let text: SharedString = parts
                                        .iter()
                                        .map(|part| part.as_ref())
                                        .collect::<String>()
                                        .into();
                                    let run = gpui::TextRun {
                                        len: text.len(),
                                        font: gpui::font(crate::platform::code_font()),
                                        color: gpui::black(),
                                        background_color: None,
                                        underline: None,
                                        strikethrough: None,
                                    };
                                    text_system.shape_line(text, px(12.), &[run], None).width
                                };
                                let mut plain_width = px(0.);
                                let mut whitespace_width = px(0.);
                                for line in rows
                                    .iter()
                                    .flat_map(|row| row.expected.iter().chain(&row.actual))
                                {
                                    plain_width = plain_width.max(measure(&line.plain));
                                    whitespace_width =
                                        whitespace_width.max(measure(&line.whitespace));
                                }
                                Content {
                                    rows,
                                    changes: diff.changes,
                                    truncated: diff.truncated,
                                    invalid_utf8: diff.invalid_utf8,
                                    plain_width,
                                    whitespace_width,
                                }
                            })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            if let Some(first) = content.changes.first() {
                                this.scroll
                                    .scroll_to_item(*first, gpui::ScrollStrategy::Top);
                            }
                            this.content = Some(content);
                            cx.notify();
                        });
                    }));
                    view
                })
            },
        );
        if let Err(error) = opened {
            self.status = StatusMessage::Document(id, format!("无法打开差异：{error:#}"));
            cx.notify();
        }
    }
}
impl DiffWindow {
    fn horizontal_limit(&self, window: &Window) -> gpui::Pixels {
        let width = self.content.as_ref().map_or(px(0.), |content| {
            if self.whitespace {
                content.whitespace_width
            } else {
                content.plain_width
            }
        });
        (width - (window.viewport_size().width / 2. - px(46.))).max(px(0.))
    }
    fn horizontal_scroll(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(24.));
        let movement = if event.modifiers.shift && delta.x == px(0.) {
            delta.y
        } else {
            delta.x
        };
        if movement != px(0.) {
            self.horizontal =
                (self.horizontal - movement).clamp(px(0.), self.horizontal_limit(window));
            cx.stop_propagation();
            cx.notify();
        }
    }
    fn navigate(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(content) = &self.content else {
            return;
        };
        if content.changes.is_empty() {
            return;
        }
        self.current = if forward {
            (self.current + 1) % content.changes.len()
        } else {
            (self.current + content.changes.len() - 1) % content.changes.len()
        };
        self.scroll
            .scroll_to_item(content.changes[self.current], gpui::ScrollStrategy::Top);
        cx.notify();
    }
    fn cell(
        &self,
        line: Option<&DisplayLine>,
        changed: bool,
        expected: bool,
        index: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        let c = crate::panel_style::palette(cx);
        let ink = if expected { c.error } else { c.success };
        div()
            .id((
                if expected {
                    "diff-left-cell"
                } else {
                    "diff-right-cell"
                },
                index,
            ))
            .debug_selector(move || {
                format!(
                    "diff-{}-cell-{index}",
                    if expected { "left" } else { "right" }
                )
            })
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .on_scroll_wheel(cx.listener(Self::horizontal_scroll))
            .when(expected, |cell| cell.border_r_1().border_color(c.line))
            .when(changed, |cell| cell.bg(ink.opacity(0.07)))
            .child(
                div()
                    .w(px(46.))
                    .flex_shrink_0()
                    .pr(px(8.))
                    .text_right()
                    .text_size(px(10.))
                    .text_color(c.muted)
                    .child(line.map(|line| line.number.clone()).unwrap_or_default()),
            )
            .child(
                div()
                    .id((
                        if expected {
                            "diff-expected"
                        } else {
                            "diff-actual"
                        },
                        index,
                    ))
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id((
                                if expected {
                                    "diff-left-text"
                                } else {
                                    "diff-right-text"
                                },
                                index,
                            ))
                            .debug_selector(move || {
                                format!(
                                    "diff-{}-text-{index}",
                                    if expected { "left" } else { "right" }
                                )
                            })
                            .relative()
                            .left(-self.horizontal)
                            .flex()
                            .whitespace_nowrap()
                            .font_family(crate::platform::code_font())
                            .text_size(px(12.))
                            .line_height(px(24.))
                            .when_some(line, |row, line| {
                                let parts = if self.whitespace {
                                    &line.whitespace
                                } else {
                                    &line.plain
                                };
                                row.child(div().flex_shrink_0().child(parts[0].clone()))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .when(changed, |part| part.bg(ink.opacity(0.20)))
                                            .child(parts[1].clone()),
                                    )
                                    .child(div().flex_shrink_0().child(parts[2].clone()))
                            }),
                    ),
            )
            .into_any_element()
    }
}
impl Render for DiffWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.horizontal = self.horizontal.min(self.horizontal_limit(window));
        let c = crate::panel_style::palette(cx);
        let count = self
            .content
            .as_ref()
            .map_or(0, |content| content.changes.len());
        div()
            .size_full()
            .on_action(|_: &CloseDocument, window, cx| {
                window.remove_window();
                cx.stop_propagation();
            })
            .flex()
            .flex_col()
            .bg(c.paper)
            .text_color(c.text)
            .child(
                div()
                    .h(px(44.))
                    .flex_shrink_0()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(c.line)
                    .child(div().text_size(px(12.)).child(self.description.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().text_size(px(11.)).text_color(c.muted).child(
                                if count > 0 {
                                    format!("{} / {} 行差异", self.current + 1, count)
                                } else {
                                    "无文本差异".into()
                                },
                            ))
                            .child(
                                crate::panel_style::button("diff-previous", "上一处", cx)
                                    .disabled(count == 0)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.navigate(false, cx)),
                                    ),
                            )
                            .child(
                                crate::panel_style::button("diff-next", "下一处", cx)
                                    .disabled(count == 0)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.navigate(true, cx)),
                                    ),
                            )
                            .child(
                                crate::panel_style::button(
                                    "diff-whitespace",
                                    if self.whitespace {
                                        "隐藏空白"
                                    } else {
                                        "显示空白"
                                    },
                                    cx,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.whitespace = !this.whitespace;
                                        cx.notify();
                                    },
                                )),
                            ),
                    ),
            )
            .when(
                self.content
                    .as_ref()
                    .is_some_and(|content| content.truncated || content.invalid_utf8),
                |view| {
                    view.child(
                        div()
                            .px(px(12.))
                            .py(px(6.))
                            .text_size(px(11.))
                            .text_color(c.muted)
                            .child(
                                if self
                                    .content
                                    .as_ref()
                                    .is_some_and(|content| content.invalid_utf8)
                                {
                                    "实际输出包含无效 UTF-8；以下为替代字符预览。"
                                } else {
                                    "大输出仅展示首个差异附近的片段；完整内容保留在执行目录。"
                                },
                            ),
                    )
                },
            )
            .child(
                div()
                    .h(px(30.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .bg(c.subtle)
                    .border_b_1()
                    .border_color(c.line)
                    .text_size(px(11.))
                    .child(div().flex_1().pl(px(12.)).child("预期输出"))
                    .child(div().flex_1().pl(px(12.)).child("实际输出")),
            )
            .child(if self.content.is_none() {
                div()
                    .p(px(16.))
                    .text_color(c.muted)
                    .child("正在比较…")
                    .into_any_element()
            } else {
                gpui::uniform_list(
                    "output-diff",
                    self.content
                        .as_ref()
                        .map_or(0, |content| content.rows.len()),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let Some(content) = &this.content else {
                            return Vec::new();
                        };
                        range
                            .map(|index| {
                                let row = &content.rows[index];
                                div()
                                    .w_full()
                                    .h(px(24.))
                                    .flex()
                                    .child(this.cell(
                                        row.expected.as_ref(),
                                        row.changed,
                                        true,
                                        index,
                                        cx,
                                    ))
                                    .child(this.cell(
                                        row.actual.as_ref(),
                                        row.changed,
                                        false,
                                        index,
                                        cx,
                                    ))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .w_full()
                .flex_1()
                .min_h_0()
                .into_any_element()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn columns_stay_aligned_and_scroll_together(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
        });
        let (view, cx) = cx.add_window_view(|_, _| {
            let make_line = |number| {
                DisplayLine::new(DiffLine {
                    number,
                    text: format!("{}\n", "long text ".repeat(100)),
                    changed: 0..1,
                })
            };
            DiffWindow {
                content: Some(Content {
                    rows: (1..=100)
                        .map(|number| Row {
                            expected: Some(make_line(number)),
                            actual: Some(make_line(number)),
                            changed: true,
                        })
                        .collect(),
                    changes: (0..100).collect(),
                    truncated: false,
                    invalid_utf8: false,
                    plain_width: px(8000.),
                    whitespace_width: px(8000.),
                }),
                scroll: UniformListScrollHandle::new(),
                whitespace: true,
                current: 0,
                description: "WrongAnswer · test fixture".into(),
                _loading: None,
                horizontal: px(0.),
            }
        });
        cx.simulate_resize(size(px(1000.), px(640.)));
        cx.run_until_parked();
        let left = cx.debug_bounds("diff-left-cell-0").unwrap();
        let right = cx.debug_bounds("diff-right-cell-0").unwrap();
        let width = cx.update(|window, _| window.viewport_size().width);
        assert!(
            (left.size.width - width / 2.).abs() <= px(1.),
            "{left:?}, viewport {width:?}"
        );
        assert!((left.right() - right.left()).abs() <= px(1.));
        let before_left = cx.debug_bounds("diff-left-text-0").unwrap();
        let before_right = cx.debug_bounds("diff-right-text-0").unwrap();
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: right.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(-80.), px(0.))),
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |view, _| view.horizontal), px(80.));
        let after_left = cx.debug_bounds("diff-left-text-0").unwrap();
        let after_right = cx.debug_bounds("diff-right-text-0").unwrap();
        assert_eq!(before_left.origin.x - after_left.origin.x, px(80.));
        assert_eq!(before_right.origin.x - after_right.origin.x, px(80.));
        view.update(cx, |view, cx| {
            view.scroll.scroll_to_item(50, gpui::ScrollStrategy::Top);
            cx.notify();
        });
        cx.run_until_parked();
        let left = cx.debug_bounds("diff-left-cell-50").unwrap();
        let right = cx.debug_bounds("diff-right-cell-50").unwrap();
        assert_eq!(left.origin.y, right.origin.y);
    }
}

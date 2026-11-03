use super::*;
use crate::text_field::{Changed, TextField};
use crate::{document::TestCase, runner::RunMode};
use gpui::{AnyElement, AppContext, Entity, div, px};
use settings::Settings;
use ui::prelude::*;

pub(super) struct TestEditors {
    input: Entity<TextField>,
    expected: Entity<TextField>,
    actual: Entity<TextField>,
    actual_run: Option<uuid::Uuid>,
    _subscriptions: Vec<Subscription>,
}
impl TestEditors {
    fn contains_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        [&self.input, &self.expected, &self.actual]
            .into_iter()
            .any(|field| field.focus_handle(cx).contains_focused(window, cx))
    }
}
#[derive(Clone, Copy)]
enum TestField {
    Input,
    Expected,
}
fn update_test_field(test: &mut TestCase, field: TestField, text: String) -> bool {
    let target = match field {
        TestField::Input => Some(&mut test.input),
        TestField::Expected => test.expected_output.as_mut(),
    };
    let Some(target) = target else {
        return false;
    };
    if *target == text {
        return false;
    }
    *target = text;
    true
}
fn display_stdout(bytes: &[u8]) -> String {
    const DISPLAY_LIMIT_BYTES: usize = 16_384;
    let mut end = bytes.len().min(DISPLAY_LIMIT_BYTES);
    if bytes.len() > DISPLAY_LIMIT_BYTES
        && let Err(error) = std::str::from_utf8(&bytes[..end])
        && error.error_len().is_none()
    {
        end = error.valid_up_to();
    }
    let mut text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    if end < bytes.len() {
        text.push_str("\n…完整输出见执行目录");
    }
    text
}
impl Ide {
    pub(super) fn ensure_test_editors(
        &mut self,
        id: DocumentId,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keep = range.start.saturating_sub(1)..range.end.saturating_add(1);
        // UniformList probes row zero for measurement before requesting visible rows.
        // Keep that probe stable; evicting its inputs here causes repeated creation/paint.
        let cache = self.test_editors.entry(id).or_default();
        if range != (0..1) {
            cache.retain(|index, view| {
                *index == 0 || keep.contains(index) || view.contains_focus(window, cx)
            });
        }
        for index in range {
            if !self
                .test_editors
                .get(&id)
                .is_some_and(|views| views.contains_key(&index))
            {
                let test = self
                    .session
                    .documents
                    .iter()
                    .find(|document| document.id == id)
                    .and_then(|document| document.metadata.tests.get(index))
                    .cloned();
                let Some(test) = test else {
                    continue;
                };
                let editors = self.create_test_editors(id, index, test, window, cx);
                self.test_editors
                    .entry(id)
                    .or_default()
                    .insert(index, editors);
            }
            let report = self.runs.get(&id).and_then(|run| run.report.as_ref());
            let run_id = report.map(|report| report.id);
            let Some(view) = self
                .test_editors
                .get_mut(&id)
                .and_then(|views| views.get_mut(&index))
            else {
                continue;
            };
            if view.actual_run == run_id {
                continue;
            }
            let text = report
                .and_then(|report| report.cases.iter().find(|case| case.index == index))
                .map(|case| display_stdout(&case.output.stdout))
                .unwrap_or_default();
            view.actual
                .update(cx, |input, cx| input.set_text(text, window, cx));
            view.actual_run = run_id;
        }
    }
    fn create_test_editors(
        &self,
        id: DocumentId,
        index: usize,
        test: TestCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TestEditors {
        let input = cx.new(|cx| TextField::multiline(test.input, false, window, cx));
        let expected = cx.new(|cx| {
            TextField::multiline(test.expected_output.unwrap_or_default(), false, window, cx)
        });
        let actual = cx.new(|cx| TextField::multiline(String::new(), true, window, cx));
        let subscriptions = [
            (input.clone(), TestField::Input),
            (expected.clone(), TestField::Expected),
        ]
        .into_iter()
        .map(|(editor, field)| {
            cx.subscribe(&editor, move |this, editor, _: &Changed, cx| {
                let text = editor.read(cx).text(cx);
                let changed = this
                    .session
                    .documents
                    .iter_mut()
                    .find(|document| document.id == id)
                    .and_then(|document| document.metadata.tests.get_mut(index))
                    .is_some_and(|test| update_test_field(test, field, text));
                if changed {
                    this.mark_changed(id);
                    cx.notify();
                }
            })
        })
        .collect();
        TestEditors {
            input,
            expected,
            actual,
            actual_run: None,
            _subscriptions: subscriptions,
        }
    }
    pub(super) fn render_test_case(
        &self,
        document: &CodeDocument,
        index: usize,
        test: &TestCase,
        cx: &Context<Self>,
    ) -> AnyElement {
        let c = crate::panel_style::palette(cx);
        let Some(view) = self
            .test_editors
            .get(&document.id)
            .and_then(|views| views.get(&index))
        else {
            return div().into_any_element();
        };
        let run = self.runs.get(&document.id);
        let result = run
            .and_then(|r| r.report.as_ref())
            .and_then(|r| r.cases.iter().find(|case| case.index == index));
        let state = if run.is_some_and(|r| r.running) {
            "运行中"
        } else if run.is_some_and(|r| r.stale) {
            "旧版本结果"
        } else {
            result.map(|r| r.verdict.label()).unwrap_or("未运行")
        };
        let field = |title: &'static str, editor: Entity<TextField>, id: &'static str| {
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .mb(px(6.))
                        .text_size(px(10.))
                        .line_height(px(12.))
                        .text_color(c.muted)
                        .child(title),
                )
                .child(
                    div()
                        .id((id, index))
                        .h(px(120.))
                        .border_1()
                        .border_color(c.line)
                        .child(editor),
                )
        };
        let selected = test.expected_output.is_some();
        let font_size = theme::ThemeSettings::get_global(cx).ui_font_size(cx);
        div()
            .h(px(224.))
            .overflow_hidden()
            .px(px(12.))
            .py(px(10.))
            .bg(c.paper)
            .text_color(c.text)
            .border_b_1()
            .border_color(c.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(27.))
                    .mb(px(7.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(format!("用例 {}", index + 1)),
                            )
                            .child(div().text_size(px(10.)).text_color(c.muted).child(state)),
                    )
                    .child(
                        div()
                            .flex()
                            .child(
                                crate::panel_style::button(("run-case", index), "运行", cx)
                                    .disabled(run.is_some_and(|r| r.running))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start_run(RunMode::Test(index), window, cx)
                                    })),
                            )
                            .child(
                                crate::panel_style::button(("delete-case", index), "删除", cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let doc = &mut this.session.documents[this.session.active];
                                        doc.metadata.tests.remove(index);
                                        let id = doc.id;
                                        this.test_editors.remove(&id);
                                        this.mark_changed(id);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(field("输入", view.input.clone(), "case-input"))
                    .child(field("实际输出 · 只读", view.actual.clone(), "case-actual"))
                    .child(field("预期输出", view.expected.clone(), "case-expected")),
            )
            .child(
                div().mt(px(7.)).child(
                    ui::ButtonLike::new(("compare-case", index))
                        .style(ButtonStyle::Transparent)
                        .height(px(20.).into())
                        .tab_index(0_isize)
                        .toggle_state(selected)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .child(
                                    div()
                                        .size(px(13.))
                                        .flex_shrink_0()
                                        .rounded(px(2.))
                                        .border_1()
                                        .border_color(if selected { c.accent } else { c.muted })
                                        .bg(if selected { c.accent } else { c.paper })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when(selected, |d| {
                                            d.child(
                                                ui::Icon::new(ui::IconName::Check)
                                                    .size(ui::IconSize::Custom(gpui::rems(
                                                        px(11.) / font_size,
                                                    )))
                                                    .color(ui::Color::Custom(c.paper)),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(c.muted)
                                        .child("比较预期输出"),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let id = this.session.documents[this.session.active].id;
                            let value = this.test_editors[&id][&index].expected.read(cx).text(cx);
                            let test = &mut this.session.documents[this.session.active]
                                .metadata
                                .tests[index];
                            test.expected_output = if test.expected_output.is_some() {
                                None
                            } else {
                                Some(value)
                            };
                            this.mark_changed(id);
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    #[gpui::test]
    fn inline_test_edits_update_only_the_owning_document(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings = settings::SettingsStore::test(cx);
            cx.set_global(settings);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
        });
        let mut first = CodeDocument::untitled();
        first.metadata.tests.push(TestCase {
            input: "original".into(),
            expected_output: Some("answer".into()),
        });
        let mut second = CodeDocument::untitled();
        second.metadata = first.metadata.clone();
        let first_id = first.id;
        let language = Arc::new(Language::new(
            toml::from_str(include_str!("../../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let directory =
            std::env::temp_dir().join(format!("altaed-inline-test-{}", uuid::Uuid::new_v4()));
        let (ide, cx) = cx.add_window_view(|window, cx| {
            Ide::new(
                Session {
                    documents: vec![first, second],
                    active: 0,
                },
                directory,
                language,
                window,
                cx,
            )
        });
        cx.update(|window, cx| {
            let (input, expected) = ide.read_with(cx, |ide, _| {
                let view = &ide.test_editors[&first_id][&0];
                (view.input.clone(), view.expected.clone())
            });
            input.update(cx, |e, cx| e.set_text("edited input\n", window, cx));
            expected.update(cx, |e, cx| e.set_text("edited answer\n", window, cx));
        });
        cx.run_until_parked();
        ide.read_with(cx, |ide, _| {
            assert_eq!(
                ide.session.documents[0].metadata.tests[0].input,
                "edited input\n"
            );
            assert_eq!(
                ide.session.documents[0].metadata.tests[0]
                    .expected_output
                    .as_deref(),
                Some("edited answer\n")
            );
            assert_eq!(ide.session.documents[1].metadata.tests[0].input, "original");
        });
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    #[gpui::test]
    fn five_hundred_cases_only_create_visible_text_fields(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let settings = settings::SettingsStore::test(cx);
            cx.set_global(settings);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            crate::text_field::init(cx);
        });
        let mut doc = CodeDocument::untitled();
        doc.metadata.tests = (0..500)
            .map(|i| TestCase {
                input: format!("{i}\n"),
                expected_output: Some(format!("{}\n", i * 2)),
            })
            .collect();
        let id = doc.id;
        let language = Arc::new(Language::new(
            toml::from_str(include_str!("../../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let directory = std::env::temp_dir().join(format!("altaed-perf-{}", uuid::Uuid::new_v4()));
        let start = std::time::Instant::now();
        let (ide, cx) = cx.add_window_view(|window, cx| {
            Ide::new(
                Session {
                    documents: vec![doc],
                    active: 0,
                },
                directory,
                language,
                window,
                cx,
            )
        });
        let count = ide.read_with(cx, |ide, _| ide.test_editors[&id].len());
        eprintln!(
            "500 cases: {} text fields, initial test render {:?}",
            count * 3,
            start.elapsed()
        );
        assert!(
            count > 0 && count < 20,
            "only visible rows should allocate inputs, got {count}"
        );
        let input = ide.read_with(cx, |ide, _| ide.test_editors[&id][&0].input.entity_id());
        ide.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        assert_eq!(
            ide.read_with(cx, |ide, _| ide.test_editors[&id][&0].input.entity_id()),
            input
        );
        let initial = ide.read_with(cx, |ide, _| ide.test_editors[&id].len());
        cx.update(|window, cx| {
            ide.update(cx, |ide, cx| {
                ide.ensure_test_editors(id, 490..495, window, cx)
            })
        });
        let count = ide.read_with(cx, |ide, _| ide.test_editors[&id].len());
        assert!(count <= initial + 5, "scrolling should evict unused rows");
    }
}

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
    _files: Option<gpui::Task<()>>,
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
pub(super) enum TestField {
    Input,
    Expected,
}
fn update_test_field(test: &mut TestCase, field: TestField, text: String) -> bool {
    if match field {
        TestField::Input => test.input_file.is_some(),
        TestField::Expected => test.expected_file.is_some(),
    } {
        return false;
    }
    match field {
        TestField::Input => {
            if test.input == text {
                return false;
            }
            test.input = text;
        }
        TestField::Expected => {
            let next = (!text.is_empty()).then_some(text);
            if test.expected_output == next {
                return false;
            }
            test.expected_output = next;
        }
    }
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
        let input_file = test.input_file.clone();
        let expected_file = test.expected_file.clone();
        let paths = self
            .session
            .documents
            .iter()
            .find(|document| document.id == id)
            .map(|document| {
                Ok((
                    input_file
                        .as_deref()
                        .map(|path| altaed_core::sample_files::resolve_path(document, path))
                        .transpose()?,
                    expected_file
                        .as_deref()
                        .map(|path| altaed_core::sample_files::resolve_path(document, path))
                        .transpose()?,
                ))
            })
            .unwrap_or_else(|| anyhow::bail!("样例所属文档已关闭"));
        let input = cx.new(|cx| {
            TextField::multiline(
                if input_file.is_some() {
                    "正在读取文件…".into()
                } else {
                    test.input
                },
                input_file.is_some(),
                window,
                cx,
            )
        });
        let expected = cx.new(|cx| {
            let mut field = TextField::multiline(
                if expected_file.is_some() {
                    "正在读取文件…".into()
                } else {
                    test.expected_output.unwrap_or_default()
                },
                expected_file.is_some(),
                window,
                cx,
            );
            field.set_placeholder("留空仅观察输出", window, cx);
            field
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
        let files = if input_file.is_some() || expected_file.is_some() {
            let input = input.clone();
            let expected = expected.clone();
            Some(cx.spawn_in(window, async move |this, cx| {
                let previews = cx
                    .background_spawn(async move {
                        paths.map(|(input, expected)| {
                            (
                                input.map(|path| altaed_core::sample_files::preview(&path)),
                                expected.map(|path| altaed_core::sample_files::preview(&path)),
                            )
                        })
                    })
                    .await;
                let _ = this.update_in(cx, |this, window, cx| {
                    let current = this
                        .test_editors
                        .get(&id)
                        .and_then(|views| views.get(&index));
                    if !current.is_some_and(|view| view.input.entity_id() == input.entity_id()) {
                        return;
                    }
                    match previews {
                        Ok((left, right)) => {
                            for (field, result) in [(input, left), (expected, right)] {
                                if let Some(result) = result {
                                    field.update(cx, |field, cx| {
                                        field.set_text(
                                            result.unwrap_or_else(|error| {
                                                format!("文件读取失败：{error:#}")
                                            }),
                                            window,
                                            cx,
                                        )
                                    });
                                }
                            }
                        }
                        Err(error) => {
                            this.status =
                                StatusMessage::Document(id, format!("样例文件无法定位：{error:#}"))
                        }
                    }
                    cx.notify();
                });
            }))
        } else {
            None
        };
        TestEditors {
            input,
            expected,
            actual,
            actual_run: None,
            _files: files,
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
        let comparison = result
            .filter(|_| !run.is_some_and(|run| run.running || run.stale))
            .and_then(|result| match result.verdict {
                crate::runner::Verdict::Passed => Some((ui::IconName::Check, c.success)),
                crate::runner::Verdict::WrongAnswer => Some((ui::IconName::Close, c.error)),
                _ => None,
            });
        let can_open_diff = matches!(
            result.map(|result| &result.verdict),
            Some(crate::runner::Verdict::WrongAnswer)
        ) && !run.is_some_and(|run| run.running)
            && run
                .and_then(|run| run.report.as_ref())
                .is_some_and(|report| {
                    report
                        .test_snapshot
                        .iter()
                        .any(|test| test.index == index && test.expected_output.is_some())
                });
        let field = |title: &'static str,
                     editor: Entity<TextField>,
                     field_id: &'static str,
                     source: Option<TestField>,
                     file: Option<&PathBuf>| {
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .h(px(16.))
                        .mb(px(4.))
                        .text_size(px(11.))
                        .line_height(px(12.))
                        .text_color(c.muted)
                        .child(title)
                        .when_some(source, |row, source| {
                            row.child(
                                crate::panel_style::button(
                                    (
                                        match source {
                                            TestField::Input => "case-input-file",
                                            TestField::Expected => "case-expected-file",
                                        },
                                        index,
                                    ),
                                    file.and_then(|file| file.file_name())
                                        .map(|name| name.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "文件…".into()),
                                    cx,
                                )
                                .size(ButtonSize::None)
                                .width(px(64.))
                                .truncate(true)
                                .label_size(ui::LabelSize::Custom(gpui::rems(
                                    px(10.) / theme::ThemeSettings::get_global(cx).ui_font_size(cx),
                                )))
                                .tooltip(ui::Tooltip::text(
                                    file.map(|file| file.display().to_string())
                                        .unwrap_or_else(|| "打开样例文件编辑器".into()),
                                ))
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.open_sample_editor(index, source, window, cx)
                                    },
                                )),
                            )
                        }),
                )
                .child(
                    div()
                        .id((field_id, index))
                        .h(px(120.))
                        .border_1()
                        .border_color(c.line)
                        .child(editor),
                )
        };
        let font_size = theme::ThemeSettings::get_global(cx).ui_font_size(cx);
        div()
            .h(px(196.))
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
                            .child(
                                div()
                                    .id(("case-verdict", index))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .text_size(px(11.))
                                    .text_color(comparison.map_or(c.muted, |(_, color)| color))
                                    .when(can_open_diff, |state| {
                                        state
                                            .cursor_pointer()
                                            .hover(|style| style.text_color(c.accent))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.show_output_diff(index, cx)
                                            }))
                                    })
                                    .when_some(comparison, |row, (icon, color)| {
                                        row.child(
                                            ui::Icon::new(icon)
                                                .size(ui::IconSize::Custom(gpui::rems(
                                                    px(12.) / font_size,
                                                )))
                                                .color(ui::Color::Custom(color)),
                                        )
                                    })
                                    .child(state),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .when(
                                result.is_some_and(|_| !run.is_some_and(|run| run.running)),
                                |row| {
                                    row.child(
                                        crate::panel_style::button(
                                            ("case-diff", index),
                                            "Diff",
                                            cx,
                                        )
                                        .disabled(
                                            !run.and_then(|run| run.report.as_ref()).is_some_and(
                                                |report| {
                                                    report.test_snapshot.iter().any(|test| {
                                                        test.index == index
                                                            && test.expected_output.is_some()
                                                    })
                                                },
                                            ),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.show_output_diff(index, cx)
                                            }),
                                        ),
                                    )
                                },
                            )
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
                    .child(field(
                        "输入",
                        view.input.clone(),
                        "case-input",
                        Some(TestField::Input),
                        test.input_file.as_ref(),
                    ))
                    .child(field(
                        "实际输出",
                        view.actual.clone(),
                        "case-actual",
                        None,
                        None,
                    ))
                    .child(field(
                        "预期输出",
                        view.expected.clone(),
                        "case-expected",
                        Some(TestField::Expected),
                        test.expected_file.as_ref(),
                    )),
            )
            .into_any_element()
    }
}

impl Ide {
    fn open_sample_editor(
        &mut self,
        index: usize,
        field: TestField,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let document = &self.session.documents[self.session.active];
        let Some(test) = document.metadata.tests.get(index) else {
            return;
        };
        let Some(view) = self
            .test_editors
            .get(&document.id)
            .and_then(|views| views.get(&index))
        else {
            return;
        };
        let target = super::sample_editor::Target {
            document: document.id,
            index,
            field,
            view: match field {
                TestField::Input => view.input.downgrade(),
                TestField::Expected => view.expected.downgrade(),
            },
            original: test.clone(),
        };
        let file = match field {
            TestField::Input => test.input_file.as_deref(),
            TestField::Expected => test.expected_file.as_deref(),
        };
        let result = file
            .map(|path| altaed_core::sample_files::resolve_path(document, path))
            .transpose()
            .and_then(|path| {
                super::sample_editor::open(
                    cx.weak_entity(),
                    target,
                    path,
                    self.directory.clone(),
                    cx,
                )
            });
        if let Err(error) = result {
            self.status =
                StatusMessage::Document(document.id, format!("无法打开样例编辑器：{error:#}"));
            cx.notify();
        }
    }
    pub(super) fn apply_sample_edit(
        &mut self,
        target: &super::sample_editor::Target,
        file: Option<PathBuf>,
        text: String,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        use anyhow::{Context as _, ensure};
        let view = self
            .test_editors
            .get(&target.document)
            .and_then(|views| views.get(&target.index))
            .context("原样例视图已变动，请重新打开样例文件")?;
        let view = match target.field {
            TestField::Input => &view.input,
            TestField::Expected => &view.expected,
        };
        ensure!(
            view.entity_id() == target.view.entity_id(),
            "样例已被删除或重新创建，请重新打开"
        );
        let document = self
            .session
            .documents
            .iter_mut()
            .find(|document| document.id == target.document)
            .context("原文档已关闭")?;
        let stored = file
            .as_deref()
            .map(|path| altaed_core::sample_files::stored_path(document, path));
        let test = document
            .metadata
            .tests
            .get_mut(target.index)
            .context("原样例已删除")?;
        ensure!(
            test == &target.original,
            "样例已在其他窗口修改，请重新打开后再应用"
        );
        match target.field {
            TestField::Input => {
                test.input_file = stored;
                if file.is_none() {
                    test.input = text;
                }
            }
            TestField::Expected => {
                test.expected_file = stored;
                if file.is_none() {
                    test.expected_output = (!text.is_empty()).then_some(text);
                }
            }
        }
        // A saved external file may be shared by several explicit bindings.
        // Refresh bound previews and invalidate their prior run results together.
        let affected: Vec<_> = self
            .session
            .documents
            .iter()
            .filter(|document| {
                document.id == target.document
                    || document
                        .metadata
                        .tests
                        .iter()
                        .any(|test| test.input_file.is_some() || test.expected_file.is_some())
            })
            .map(|document| document.id)
            .collect();
        for id in affected {
            self.test_editors.remove(&id);
            self.mark_changed(id);
        }
        self.status = StatusMessage::Document(
            target.document,
            if file.is_some() {
                "样例文件已绑定".into()
            } else {
                "已转为可编辑文本样例".into()
            },
        );
        cx.notify();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    #[test]
    fn expected_output_automatically_enables_and_disables_comparison() {
        let mut test = TestCase::default();
        assert!(update_test_field(
            &mut test,
            TestField::Expected,
            "42".into()
        ));
        assert_eq!(test.expected_output.as_deref(), Some("42"));
        assert!(update_test_field(
            &mut test,
            TestField::Expected,
            String::new()
        ));
        assert_eq!(test.expected_output, None);
        assert!(update_test_field(
            &mut test,
            TestField::Expected,
            "\n".into()
        ));
        assert_eq!(test.expected_output.as_deref(), Some("\n"));
    }
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
            ..Default::default()
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
        let file =
            std::env::temp_dir().join(format!("altaed-bound-case-{}.in", uuid::Uuid::new_v4()));
        std::fs::write(&file, "file input\n").unwrap();
        let target = ide.read_with(cx, |ide, _| super::super::sample_editor::Target {
            document: first_id,
            index: 0,
            field: TestField::Input,
            view: ide.test_editors[&first_id][&0].input.downgrade(),
            original: ide.session.documents[0].metadata.tests[0].clone(),
        });
        cx.update(|window, cx| {
            ide.update(cx, |ide, cx| {
                ide.apply_sample_edit(&target, Some(file.clone()), String::new(), window, cx)
                    .unwrap();
                assert!(
                    ide.apply_sample_edit(&target, None, "stale popup".into(), window, cx)
                        .is_err()
                );
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let input = ide.read(cx).test_editors[&first_id][&0].input.clone();
            assert_eq!(input.read(cx).text(cx), "file input\n");
            input.update(cx, |field, cx| {
                field.set_text("programmatic preview", window, cx)
            });
        });
        cx.run_until_parked();
        ide.read_with(cx, |ide, _| {
            assert_eq!(
                ide.session.documents[0].metadata.tests[0].input,
                "edited input\n"
            );
            assert_eq!(
                ide.session.documents[0].metadata.tests[0]
                    .input_file
                    .as_ref(),
                Some(&file)
            );
            assert!(
                ide.session.documents[1].metadata.tests[0]
                    .input_file
                    .is_none()
            );
        });
        std::fs::remove_file(file).unwrap();
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
                ..Default::default()
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

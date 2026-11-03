//! Transient form state belongs to one document. Only valid values enter its
//! execution settings; incomplete edits stay visible and block sample runs.
use super::*;
use crate::text_field::{Changed, TextField};
use gpui::{Entity, EntityId};

#[derive(Default)]
pub(crate) struct CheckerControls {
    pub expanded: bool,
    form: Option<ParameterForm>,
}
struct ParameterForm {
    applied: Checker,
    fields: Vec<(Parameter, Entity<TextField>)>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Parameter {
    Absolute,
    Relative,
    Timeout,
    Arguments,
}
impl Parameter {
    fn label(self) -> &'static str {
        match self {
            Self::Absolute => "绝对误差",
            Self::Relative => "相对误差",
            Self::Timeout => "超时（ms）",
            Self::Arguments => "附加参数（每行一个）",
        }
    }
    fn selector(self) -> &'static str {
        match self {
            Self::Absolute => "checker-absolute",
            Self::Relative => "checker-relative",
            Self::Timeout => "checker-timeout",
            Self::Arguments => "checker-arguments",
        }
    }
}
impl ParameterForm {
    fn new(id: DocumentId, checker: Checker, window: &mut Window, cx: &mut Context<Ide>) -> Self {
        let values = match &checker {
            Checker::Float { absolute, relative } => vec![
                (Parameter::Absolute, absolute.to_string()),
                (Parameter::Relative, relative.to_string()),
            ],
            Checker::Executable(checker) => vec![
                (Parameter::Timeout, checker.timeout_ms.to_string()),
                (Parameter::Arguments, checker.arguments.join("\n")),
            ],
            _ => Vec::new(),
        };
        let fields: Vec<_> = values
            .into_iter()
            .map(|(parameter, value)| {
                let field = cx.new(|cx| {
                    let mut field = if parameter == Parameter::Arguments {
                        TextField::compact_multiline(value, window, cx)
                    } else {
                        TextField::single_line(value, window, cx)
                    };
                    field.set_padding_xy(8., 4.);
                    if parameter != Parameter::Arguments {
                        field.use_ui_font(window, cx);
                    }
                    field
                });
                (parameter, field)
            })
            .collect();
        let subscriptions = fields
            .iter()
            .map(|(parameter, field)| {
                let parameter = *parameter;
                cx.subscribe(field, move |ide, field, _: &Changed, cx| {
                    ide.checker_parameter_edited(id, parameter, field.entity_id(), cx)
                })
            })
            .collect();
        Self {
            applied: checker,
            fields,
            error: None,
            _subscriptions: subscriptions,
        }
    }
    fn parsed(&self, cx: &gpui::App) -> anyhow::Result<Checker> {
        use anyhow::Context as _;
        let value = |parameter| {
            self.fields
                .iter()
                .find(|(kind, _)| *kind == parameter)
                .map(|(_, field)| field.read(cx).text(cx))
                .context("比较器参数控件不可用")
        };
        let checker = match &self.applied {
            Checker::Float { .. } => Checker::Float {
                absolute: value(Parameter::Absolute)?
                    .trim()
                    .parse()
                    .context("绝对误差必须是非负有限数")?,
                relative: value(Parameter::Relative)?
                    .trim()
                    .parse()
                    .context("相对误差必须是非负有限数")?,
            },
            Checker::Executable(original) => Checker::Executable(ExecutableChecker {
                program: original.program.clone(),
                timeout_ms: value(Parameter::Timeout)?
                    .trim()
                    .parse()
                    .context("超时必须是大于零的整数")?,
                arguments: value(Parameter::Arguments)?
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect(),
            }),
            checker => checker.clone(),
        };
        checker.validate()?;
        Ok(checker)
    }
}
impl Ide {
    /// Rebuild only when an external settings change replaces this checker's
    /// value. Ordinary rerenders preserve the form, cursor, and incomplete input.
    pub(in crate::app) fn sync_checker_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checker_controls.retain(|id, _| {
            self.session
                .documents
                .iter()
                .any(|document| document.id == *id)
        });
        let document = &self.session.documents[self.session.active];
        let id = document.id;
        let checker = document.metadata.settings.execution.checker.clone();
        if let Some(controls) = self.checker_controls.get_mut(&id)
            && controls.expanded
            && controls
                .form
                .as_ref()
                .is_none_or(|form| form.applied != checker)
        {
            controls.form = Some(ParameterForm::new(id, checker, window, cx));
        }
    }
    pub(super) fn toggle_checker_options(
        &mut self,
        id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let controls = self.checker_controls.entry(id).or_default();
        controls.expanded = !controls.expanded;
        self.sync_checker_controls(window, cx);
        cx.notify();
    }
    fn checker_parameter_edited(
        &mut self,
        id: DocumentId,
        parameter: Parameter,
        source: EntityId,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self
            .checker_controls
            .get(&id)
            .and_then(|controls| controls.form.as_ref())
        else {
            return;
        };
        if !form
            .fields
            .iter()
            .any(|(kind, field)| *kind == parameter && field.entity_id() == source)
        {
            return;
        }
        let Some(document) = self
            .session
            .documents
            .iter_mut()
            .find(|document| document.id == id)
        else {
            return;
        };
        if document.metadata.settings.execution.checker != form.applied {
            return;
        }
        let parsed = form.parsed(cx);
        let form = self
            .checker_controls
            .get_mut(&id)
            .and_then(|controls| controls.form.as_mut());
        let Some(form) = form else {
            return;
        };
        match parsed {
            Ok(checker) => {
                let changed = document.metadata.settings.execution.checker != checker;
                document.metadata.settings.execution.checker = checker.clone();
                form.applied = checker;
                form.error = None;
                if changed {
                    self.mark_changed(id);
                }
            }
            Err(error) => form.error = Some(format!("{error:#}")),
        }
        cx.notify();
    }
    pub(in crate::app) fn checker_parameter_error(&self, id: DocumentId) -> Option<&str> {
        let form = self.checker_controls.get(&id)?.form.as_ref()?;
        let document = self
            .session
            .documents
            .iter()
            .find(|document| document.id == id)?;
        (document.metadata.settings.execution.checker == form.applied)
            .then_some(form.error.as_deref())
            .flatten()
    }
    pub(super) fn render_checker_parameters(
        &self,
        document: &CodeDocument,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let controls = self.checker_controls.get(&document.id)?;
        if !controls.expanded {
            return None;
        }
        let form = controls.form.as_ref()?;
        let colors = crate::panel_style::palette(cx);
        let mut content = div().flex().flex_col().gap(gpui::px(10.));
        if let Checker::Executable(checker) = &form.applied {
            let path = checker.program.display().to_string();
            content = content.child(
                div()
                    .id("checker-program-name")
                    .truncate()
                    .text_color(colors.text)
                    .child(path.clone())
                    .tooltip(ui::Tooltip::text(path)),
            );
        }
        if form.fields.is_empty() {
            content = content.child(checker_description(&form.applied));
        } else {
            content = content.child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(gpui::px(16.))
                    .children(form.fields.iter().map(|(parameter, field)| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(gpui::px(5.))
                            .child(parameter.label())
                            .child(
                                div()
                                    .id(parameter.selector())
                                    .debug_selector({
                                        let selector = parameter.selector();
                                        move || selector.into()
                                    })
                                    .h(gpui::px(if *parameter == Parameter::Arguments {
                                        50.
                                    } else {
                                        30.
                                    }))
                                    .w_full()
                                    .border_1()
                                    .border_color(colors.line)
                                    .rounded(gpui::px(2.))
                                    .child(field.clone()),
                            )
                    })),
            );
        }
        if let Some(error) = &form.error {
            content = content.child(div().text_color(colors.error).child(error.clone()));
        }
        Some(
            div()
                .id("checker-parameters")
                .debug_selector(|| "checker-parameters".into())
                .w_full()
                .max_h(gpui::px(210.))
                .flex_shrink_0()
                .overflow_y_scroll()
                .p(gpui::px(12.))
                .bg(colors.readonly)
                .border_b_1()
                .border_color(colors.line)
                .text_size(gpui::px(11.))
                .line_height(gpui::px(18.))
                .text_color(colors.muted)
                .child(content)
                .into_any_element(),
        )
    }
}
fn checker_description(checker: &Checker) -> String {
    match checker {
        Checker::Trimmed => "忽略行末空格、Tab 和文末换行，保留行中空格与中间空行。".into(),
        Checker::Exact => "逐字节比较，行末空格和末尾换行均参与比较。".into(),
        Checker::Integers => "按有符号 64 位整数比较，不经过浮点转换。".into(),
        Checker::Tokens => "逐个比较由空白分隔的字符串，区分大小写。".into(),
        Checker::YesNo => "只接受 YES 和 NO，比较时不区分大小写。".into(),
        Checker::Real(precision) => format!(
            "比较有限浮点数，最大绝对误差或相对误差为 {:e}。",
            precision.epsilon()
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn inline_parameters_preserve_document_scope_and_block_invalid_runs(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            crate::text_field::init(cx);
        });
        let mut first = CodeDocument::untitled();
        first.metadata.settings.execution.checker = Checker::float_default();
        let id = first.id;
        let mut second = CodeDocument::untitled();
        second.metadata.settings.execution.checker = Checker::Exact;
        let language = Arc::new(language::Language::new(
            toml::from_str(include_str!("../../../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let directory =
            std::env::temp_dir().join(format!("altaed-checker-inline-{}", uuid::Uuid::new_v4()));
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
        cx.simulate_resize(gpui::size(px(1250.), px(800.)));
        ide.update_in(cx, |ide, window, cx| {
            ide.toggle_checker_options(id, window, cx)
        });
        cx.run_until_parked();
        let toolbar = cx.debug_bounds("checker-toolbar").unwrap();
        let selector = cx.debug_bounds("checker-selector").unwrap();
        let options = cx.debug_bounds("checker-parameters").unwrap();
        assert_eq!(toolbar.size.height, px(41.));
        assert_eq!(selector.size.height, px(26.));
        assert!(selector.left() > toolbar.left() && selector.right() < toolbar.right());
        assert!((options.top() - toolbar.bottom()).abs() <= px(1.));
        let absolute = ide.read_with(cx, |ide, _| {
            ide.checker_controls[&id].form.as_ref().unwrap().fields[0]
                .1
                .clone()
        });
        cx.update(|window, cx| absolute.update(cx, |field, cx| field.set_text("-1", window, cx)));
        cx.run_until_parked();
        ide.update_in(cx, |ide, window, cx| {
            assert!(ide.checker_parameter_error(id).is_some());
            ide.start_run(crate::runner::RunMode::Tests, window, cx);
            assert!(
                !ide.runs.contains_key(&id),
                "invalid visible input must not run the last valid checker"
            );
            ide.activate_document(1, window, cx);
        });
        cx.update(|window, cx| absolute.update(cx, |field, cx| field.set_text("0.01", window, cx)));
        cx.run_until_parked();
        ide.read_with(cx, |ide, _| {
            assert_eq!(ide.session.active, 1);
            assert!(matches!(ide.session.documents[0].metadata.settings.execution.checker, Checker::Float { absolute, .. } if absolute == 0.01));
            assert_eq!(ide.session.documents[1].metadata.settings.execution.checker, Checker::Exact);
            assert!(ide.checker_parameter_error(id).is_none());
        });
        ide.update_in(cx, |ide, window, cx| {
            ide.activate_document(0, window, cx);
            let original = ide.session.documents[0].metadata.settings.execution.clone();
            let mut next = original.clone();
            next.checker = Checker::Trimmed;
            ide.apply_checker_settings(id, &original, next, cx).unwrap();
        });
        cx.run_until_parked();
        cx.update(|window, cx| absolute.update(cx, |field, cx| field.set_text("9", window, cx)));
        cx.run_until_parked();
        ide.read_with(cx, |ide, _| {
            assert_eq!(
                ide.session.documents[0].metadata.settings.execution.checker,
                Checker::Trimmed
            )
        });
    }
}

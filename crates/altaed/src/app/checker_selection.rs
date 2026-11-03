//! Per-document test comparison controls. File selection and validation never
//! retarget a different tab if the user switches documents while the picker is open.
mod parameters;
use super::*;
use crate::checker::{Checker, ExecutableChecker};
use crate::runner::ExecutionSettings;
use gpui::{AnyElement, div, px};
pub(super) use parameters::CheckerControls;
use settings::Settings;
use theme::ThemeSettings;
use ui::{ButtonLike, ContextMenu, PopoverMenu, prelude::*};

impl Ide {
    pub(super) fn render_checker_controls(
        &self,
        document: &CodeDocument,
        cx: &Context<Self>,
    ) -> AnyElement {
        let colors = crate::panel_style::palette(cx);
        let id = document.id;
        let original = document.metadata.settings.execution.clone();
        let expanded = self
            .checker_controls
            .get(&id)
            .is_some_and(|controls| controls.expanded);
        let font_size = ThemeSettings::get_global(cx).ui_font_size(cx);
        let small_label = LabelSize::Custom(gpui::rems(px(11.) / font_size));
        let icon_size = IconSize::Custom(gpui::rems(px(14.) / font_size));
        div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("checker-toolbar")
                    .debug_selector(|| "checker-toolbar".into())
                    .h(px(41.))
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(12.))
                    .pr(px(8.))
                    .border_b_1()
                    .border_color(colors.line)
                    .bg(colors.paper)
                    .text_size(px(11.))
                    .text_color(colors.muted)
                    .child(div().flex_shrink_0().child("比较器"))
                    .child(
                        div()
                            .id("checker-selector")
                            .debug_selector(|| "checker-selector".into())
                            .flex_1()
                            .min_w_0()
                            .h(px(26.))
                            .border_1()
                            .border_color(colors.line)
                            .rounded(px(2.))
                            .child(self.checker_menu(document, cx)),
                    )
                    .child(
                        crate::panel_style::button("import-test-checker", "导入", cx)
                            .label_size(small_label)
                            .icon(IconName::Folder)
                            .icon_position(IconPosition::Start)
                            .icon_size(icon_size)
                            .tooltip(ui::Tooltip::text("导入现成可执行比较器"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.import_checker(id, original.clone(), window, cx)
                            })),
                    )
                    .child(
                        IconButton::new("checker-options-toggle", IconName::Sliders)
                            .style(ButtonStyle::Transparent)
                            .size(ButtonSize::None)
                            .width(px(26.))
                            .icon_size(icon_size)
                            .icon_color(Color::Custom(if expanded {
                                colors.accent
                            } else {
                                colors.muted
                            }))
                            .tooltip(ui::Tooltip::text("比较器设置"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_checker_options(id, window, cx)
                            })),
                    ),
            )
            .children(self.render_checker_parameters(document, cx))
            .into_any_element()
    }
    fn checker_menu(&self, document: &CodeDocument, cx: &Context<Self>) -> impl IntoElement {
        let id = document.id;
        let original = document.metadata.settings.execution.clone();
        let owner = cx.weak_entity();
        let colors = crate::panel_style::palette(cx);
        let font_size = ThemeSettings::get_global(cx).ui_font_size(cx);
        // Keep the selector's full rectangle clickable; only its text truncates.
        // The parent supplies the border so Transparent never adds a hover fill.
        PopoverMenu::new("test-checker-menu")
            .full_width(true)
            .trigger(
                ButtonLike::new("test-checker")
                    .style(ButtonStyle::Transparent)
                    .size(ButtonSize::None)
                    .height(px(24.).into())
                    .full_width()
                    .tab_index(0_isize)
                    .tooltip(ui::Tooltip::text(original.checker_label()))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .px(px(7.))
                            .text_size(px(11.))
                            .text_color(colors.text)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_left()
                                    .truncate()
                                    .child(original.checker_label()),
                            )
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .size(IconSize::Custom(gpui::rems(px(14.) / font_size)))
                                    .color(Color::Custom(colors.muted)),
                            ),
                    ),
            )
            .menu(move |window, cx| {
                let owner = owner.clone();
                let original = original.clone();
                Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                    for (_, checker) in Checker::builtins() {
                        let label = checker.label();
                        let selected = original.checker == checker;
                        let owner = owner.clone();
                        let mut next = original.clone();
                        next.checker = checker;
                        let baseline = original.clone();
                        menu = menu.toggleable_entry(
                            label,
                            selected,
                            IconPosition::Start,
                            None,
                            move |_, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if let Err(error) =
                                        this.apply_checker_settings(id, &baseline, next.clone(), cx)
                                    {
                                        this.status = StatusMessage::Document(
                                            id,
                                            format!("比较方式未修改：{error:#}"),
                                        );
                                        cx.notify();
                                    }
                                });
                            },
                        );
                    }
                    let mut additional = Vec::new();
                    if let Checker::Float { .. } = &original.checker {
                        additional.push(original.checker.clone());
                    } else {
                        additional.push(Checker::float_default());
                    }
                    if matches!(&original.checker, Checker::Executable(_)) {
                        additional.push(original.checker.clone());
                    }
                    for checker in additional {
                        let label = checker.label();
                        let selected = original.checker == checker;
                        let owner = owner.clone();
                        let baseline = original.clone();
                        let mut next = original.clone();
                        next.checker = checker;
                        menu = menu.toggleable_entry(
                            label,
                            selected,
                            IconPosition::Start,
                            None,
                            move |_, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if let Err(error) =
                                        this.apply_checker_settings(id, &baseline, next.clone(), cx)
                                    {
                                        this.status = StatusMessage::Document(
                                            id,
                                            format!("比较方式未修改：{error:#}"),
                                        );
                                        cx.notify();
                                    }
                                });
                            },
                        );
                    }
                    menu
                }))
            })
            .anchor(gpui::Corner::TopLeft)
    }
    fn apply_checker_settings(
        &mut self,
        id: DocumentId,
        original: &ExecutionSettings,
        next: ExecutionSettings,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        use anyhow::{Context as _, ensure};
        next.validate()?;
        let document = self
            .session
            .documents
            .iter_mut()
            .find(|document| document.id == id)
            .context("所属文档已关闭")?;
        ensure!(
            &document.metadata.settings.execution == original,
            "比较设置已被其他操作修改，请重新选择"
        );
        document.metadata.settings.execution = next;
        self.mark_changed(id);
        cx.notify();
        Ok(())
    }
    fn import_checker(
        &mut self,
        id: DocumentId,
        original: ExecutionSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("导入比较器".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let Some(path) = picker.await??.and_then(|paths| paths.into_iter().next()) else {
                    return anyhow::Ok(None);
                };
                cx.background_spawn(
                    async move { ExecutableChecker::validate_program(&path).map(Some) },
                )
                .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                let result = result.and_then(|path| {
                    if let Some(path) = path {
                        let document = this
                            .session
                            .documents
                            .iter()
                            .find(|document| document.id == id)
                            .ok_or_else(|| anyhow::anyhow!("所属文档已关闭"))?;
                        let mut next = original.clone();
                        next.checker = Checker::Executable(ExecutableChecker {
                            program: document.stored_attachment_path(&path),
                            ..Default::default()
                        });
                        this.apply_checker_settings(id, &original, next, cx)?;
                        this.status = StatusMessage::Document(
                            id,
                            "比较器已导入，下次运行测试时使用。".into(),
                        );
                    }
                    anyhow::Ok(())
                });
                if let Err(error) = result {
                    this.status = StatusMessage::General(format!("比较器未导入：{error:#}"));
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn comparison_selection_stays_with_its_document_and_rejects_stale_imports(
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
        let first = CodeDocument::untitled();
        let id = first.id;
        let original = first.metadata.settings.execution.clone();
        let second = CodeDocument::untitled();
        let language = Arc::new(language::Language::new(
            toml::from_str(include_str!("../../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let directory =
            std::env::temp_dir().join(format!("altaed-checker-selection-{}", uuid::Uuid::new_v4()));
        let (ide, _) = cx.add_window_view(|window, cx| {
            Ide::new(
                Session {
                    documents: vec![first, second],
                    active: 1,
                },
                directory,
                language,
                window,
                cx,
            )
        });
        ide.update(cx, |this, cx| {
            let mut next = original.clone();
            next.checker = Checker::Executable(ExecutableChecker {
                program: "custom checker".into(),
                ..Default::default()
            });
            this.apply_checker_settings(id, &original, next.clone(), cx)
                .unwrap();
            assert_eq!(this.session.active, 1);
            assert_eq!(this.session.documents[0].metadata.settings.execution, next);
            assert_eq!(
                this.session.documents[1].metadata.settings.execution,
                original
            );
            assert!(
                this.apply_checker_settings(id, &original, original.clone(), cx)
                    .is_err()
            );
            this.session.documents.remove(0);
            this.session.active = 0;
            assert!(
                this.apply_checker_settings(id, &next, original.clone(), cx)
                    .is_err()
            );
            assert_eq!(
                this.session.documents[0].metadata.settings.execution,
                original
            );
        });
    }
}

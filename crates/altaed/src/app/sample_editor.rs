//! Plain-text sample file dialog. Disk reads/writes stay on the worker; binding
//! back to a case validates the original editor identity and case snapshot.
use super::test_editors::TestField;
use super::*;
use crate::text_field::{Changed, TextField};
use altaed_core::sample_files::{self, TextFile};
use anyhow::Context as _;
use gpui::{AppContext, Bounds, Entity, WeakEntity, WindowBounds, WindowOptions, div, px, size};
use ui::prelude::*;

#[derive(Clone)]
pub(super) struct Target {
    pub document: DocumentId,
    pub index: usize,
    pub field: TestField,
    pub view: WeakEntity<TextField>,
    pub original: crate::document::TestCase,
}
struct Loaded {
    path: PathBuf,
    text: String,
    original: Option<TextFile>,
    preview_only: bool,
}
fn load(path: PathBuf) -> anyhow::Result<Loaded> {
    let path =
        std::fs::canonicalize(&path).with_context(|| format!("无法打开 {}", path.display()))?;
    let preview_only = std::fs::metadata(&path)?.len() > sample_files::MAX_EDITOR_BYTES;
    let original = if preview_only {
        None
    } else {
        Some(sample_files::read_text(
            &path,
            sample_files::MAX_EDITOR_BYTES,
        )?)
    };
    let text = if let Some(original) = &original {
        original.text.clone()
    } else {
        sample_files::preview(&path)?
    };
    Ok(Loaded {
        path,
        text,
        original,
        preview_only,
    })
}
#[derive(Clone, Copy)]
enum SaveKind {
    Current,
    As,
}
struct SampleEditor {
    parent: WeakEntity<Ide>,
    target: Target,
    input: Entity<TextField>,
    _changed: Subscription,
    path: Option<PathBuf>,
    original: Option<TextFile>,
    saved: String,
    saved_revision: usize,
    preview_only: bool,
    ready: bool,
    busy: bool,
    message: String,
    _operation: Option<gpui::Task<()>>,
    directory: PathBuf,
}
#[derive(Default)]
struct SampleEditors {
    windows: Vec<gpui::WindowHandle<SampleEditor>>,
}
impl gpui::Global for SampleEditors {}
pub(super) fn prevent_quit(cx: &mut App) -> bool {
    if !cx.has_global::<SampleEditors>() {
        return false;
    }
    let windows = cx.global::<SampleEditors>().windows.clone();
    for handle in windows {
        if handle
            .update(cx, |editor, window, cx| {
                if editor.busy {
                    editor.message = "文件操作尚未完成，请稍后再退出。".into();
                    cx.notify();
                    return true;
                }
                if editor.dirty(cx) && editor.input.read(cx).text(cx) != editor.saved {
                    editor.should_close(window, cx);
                    return true;
                }
                false
            })
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}
pub(super) fn open(
    parent: WeakEntity<Ide>,
    target: Target,
    path: Option<PathBuf>,
    directory: PathBuf,
    cx: &mut Context<Ide>,
) -> anyhow::Result<()> {
    let bounds = Bounds::centered(None, size(px(880.), px(560.)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(600.), px(360.))),
            ..Default::default()
        },
        move |window, cx| {
            theme::setup_ui_font(window, cx);
            let view = cx.new(|cx| {
                let text = match target.field {
                    TestField::Input => target.original.input.clone(),
                    TestField::Expected => {
                        target.original.expected_output.clone().unwrap_or_default()
                    }
                };
                let input = cx.new(|cx| TextField::document(text.clone(), window, cx));
                window.focus(&input.focus_handle(cx), cx);
                let saved_revision = input.read(cx).revision(cx);
                let changed = cx.subscribe(&input, |_, _, _: &Changed, cx| cx.notify());
                let mut view = SampleEditor {
                    parent,
                    target,
                    input,
                    _changed: changed,
                    path: None,
                    original: None,
                    saved: text,
                    saved_revision,
                    preview_only: false,
                    ready: true,
                    busy: false,
                    message: String::new(),
                    _operation: None,
                    directory,
                };
                if let Some(path) = path {
                    view.load_path(path, window, cx);
                }
                view
            });
            let weak = view.downgrade();
            window.on_window_should_close(cx, move |window, cx| {
                weak.update(cx, |view, cx| view.should_close(window, cx))
                    .unwrap_or(true)
            });
            view
        },
    )?;
    cx.default_global::<SampleEditors>().windows.push(handle);
    Ok(())
}
impl SampleEditor {
    fn dirty(&self, cx: &App) -> bool {
        self.input.read(cx).revision(cx) != self.saved_revision
    }
    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.busy {
            return false;
        }
        if !self.dirty(cx) || self.input.read(cx).text(cx) == self.saved {
            return true;
        }
        let choice = window.prompt(
            gpui::PromptLevel::Warning,
            "放弃未保存的样例文件修改？",
            None,
            &["放弃修改", "取消"],
            cx,
        );
        self.busy = true;
        self._operation = Some(cx.spawn_in(window, async move |this, cx| {
            let discard = choice.await.ok() == Some(0);
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                if discard {
                    window.remove_window();
                } else {
                    cx.notify();
                }
            });
        }));
        false
    }
    fn load_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        self.message = "正在读取文件…".into();
        self.input
            .update(cx, |input, cx| input.set_read_only(true, cx));
        self._operation = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { load(path) }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(file) => this.loaded(file, window, cx),
                    Err(error) => {
                        this.ready = false;
                        this.message = format!("无法读取：{error:#}");
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn loaded(&mut self, file: Loaded, window: &mut Window, cx: &mut Context<Self>) {
        self.path = Some(file.path);
        self.original = file.original;
        self.saved = file.text.clone();
        self.preview_only = file.preview_only;
        self.ready = true;
        self.message = if self.preview_only {
            "大文件仅显示预览；绑定后运行使用完整文件。".into()
        } else {
            String::new()
        };
        self.input.update(cx, |input, cx| {
            input.set_text(file.text, window, cx);
            input.set_read_only(self.preview_only, cx);
        });
        self.saved_revision = self.input.read(cx).revision(cx);
    }
    fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let confirmation = self.dirty(cx).then(|| {
            window.prompt(
                gpui::PromptLevel::Warning,
                "打开其他文件将放弃当前修改",
                None,
                &["继续打开", "取消"],
                cx,
            )
        });
        self.busy = true;
        self._operation = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(confirmation) = confirmation
                && confirmation.await.ok() != Some(0)
            {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                });
                return;
            }
            let selection = cx.update(|_, cx| {
                cx.prompt_for_paths(gpui::PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: None,
                })
            });
            let result = async {
                let Some(paths) = selection?.await?? else {
                    return anyhow::Ok(None);
                };
                let Some(path) = paths.into_iter().next() else {
                    return Ok(None);
                };
                cx.background_spawn(async move { load(path).map(Some) })
                    .await
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(Some(file)) => this.loaded(file, window, cx),
                    Ok(None) => (),
                    Err(error) => this.message = format!("无法打开：{error:#}"),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn apply_to_case(
        &mut self,
        file: Option<PathBuf>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bound = file.is_some();
        let result = self
            .parent
            .update(cx, |parent, cx| {
                parent.apply_sample_edit(&self.target, file, text, window, cx)
            })
            .and_then(|result| result);
        match result {
            Ok(()) => window.remove_window(),
            Err(error) => {
                self.message = format!(
                    "样例未更新：{error:#}{}",
                    if bound {
                        "。已保存的文件仍保留在当前路径。"
                    } else {
                        ""
                    }
                );
                cx.notify();
            }
        }
    }
    fn save(&mut self, kind: SaveKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.ready {
            return;
        }
        if self.preview_only {
            self.apply_to_case(self.path.clone(), String::new(), window, cx);
            return;
        }
        let text = self.input.read(cx).text(cx);
        let selection = if matches!(kind, SaveKind::As) || self.path.is_none() {
            Some(
                cx.prompt_for_new_path(
                    self.path
                        .as_deref()
                        .and_then(|path| path.parent())
                        .unwrap_or(&self.directory),
                    Some(match self.target.field {
                        TestField::Input => "sample.in",
                        TestField::Expected => "sample.out",
                    }),
                ),
            )
        } else {
            None
        };
        let previous = self.path.clone();
        let original = self.original.clone();
        let fresh = selection.is_some();
        self.busy = true;
        self._operation = Some(cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let path = if let Some(selection) = selection {
                    let Some(path) = selection.await?? else {
                        return anyhow::Ok(None);
                    };
                    path
                } else {
                    previous.context("请先选择保存路径")?
                };
                cx.background_spawn(async move {
                    let original = if fresh {
                        if path.try_exists()? {
                            Some(sample_files::read_text(
                                &path,
                                sample_files::MAX_EDITOR_BYTES,
                            )?)
                        } else {
                            None
                        }
                    } else {
                        original
                    };
                    let saved = sample_files::save_text(&path, original.as_ref(), &text)?;
                    anyhow::Ok(Some(Loaded {
                        path,
                        text,
                        original: Some(saved),
                        preview_only: false,
                    }))
                })
                .await
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(Some(file)) => {
                        let path = file.path.clone();
                        let text = file.text.clone();
                        this.loaded(file, window, cx);
                        this.apply_to_case(Some(path), text, window, cx);
                    }
                    Ok(None) => (),
                    Err(error) => this.message = format!("未保存：{error:#}"),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
impl Render for SampleEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = crate::panel_style::palette(cx);
        let dirty = self.dirty(cx);
        window.set_window_title(&format!(
            "{}文件{} — altaed",
            match self.target.field {
                TestField::Input => "输入",
                TestField::Expected => "预期输出",
            },
            if dirty { " *" } else { "" }
        ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(c.paper)
            .text_color(c.text)
            .on_action(cx.listener(|this, _: &OpenDocument, window, cx| this.open_file(window, cx)))
            .on_action(cx.listener(|this, _: &SaveDocument, window, cx| {
                this.save(SaveKind::Current, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SaveAs, window, cx| this.save(SaveKind::As, window, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseDocument, window, cx| {
                if this.should_close(window, cx) {
                    window.remove_window();
                }
            }))
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_ellipsis()
                            .child(
                                self.path
                                    .as_ref()
                                    .map(|path| path.display().to_string())
                                    .unwrap_or_else(|| "从当前样例创建文本文件".into()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                crate::panel_style::button("sample-open", "打开…", cx)
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_file(window, cx)
                                    })),
                            )
                            .child(
                                crate::panel_style::button("sample-save-as", "另存为…", cx)
                                    .disabled(self.busy || self.preview_only || !self.ready)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save(SaveKind::As, window, cx)
                                    })),
                            ),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.input.clone()))
            .when(!self.message.is_empty(), |view| {
                view.child(
                    div()
                        .px(px(12.))
                        .py(px(8.))
                        .text_size(px(11.))
                        .text_color(c.muted)
                        .child(self.message.clone()),
                )
            })
            .child(
                div()
                    .h(px(48.))
                    .flex_shrink_0()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(c.line)
                    .child(
                        crate::panel_style::button("sample-inline", "转为文本样例", cx)
                            .disabled(self.busy || self.preview_only || !self.ready)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let text = this.input.read(cx).text(cx);
                                this.apply_to_case(None, text, window, cx);
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(12.))
                            .child(
                                crate::panel_style::button("sample-cancel", "取消", cx)
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if this.should_close(window, cx) {
                                            window.remove_window();
                                        }
                                    })),
                            )
                            .child(
                                crate::panel_style::button(
                                    "sample-save",
                                    if self.preview_only {
                                        "绑定此文件"
                                    } else {
                                        "保存并绑定"
                                    },
                                    cx,
                                )
                                .disabled(self.busy || !self.ready)
                                .color(ui::Color::Custom(c.accent))
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.save(SaveKind::Current, window, cx),
                                )),
                            ),
                    ),
            )
    }
}

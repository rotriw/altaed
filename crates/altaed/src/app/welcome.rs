mod view;
use super::startup::Startup;
use crate::{
    configuration::{
        self,
        commands::{CommandDefinition, CommandId},
    },
    runner,
    text_field::{Changed, TextField},
};
use anyhow::Result;
use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, PathPromptOptions,
    Subscription, Window, WindowBounds, WindowOptions, px, size,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const EXECUTION_COMMANDS: [(&str, &str); 3] = [
    ("run.compile", "生成程序"),
    ("run.run", "启动上次编译的程序"),
    ("run.compile_and_run", "先编译，再启动"),
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Welcome,
    Environment,
    Shortcuts,
}
#[derive(Clone, Deserialize)]
struct Layout {
    width: f32,
    height: f32,
    footer: f32,
    padding: f32,
    control_height: f32,
    table_row: f32,
    breath_duration_seconds: u64,
    wordmark: WordmarkMetrics,
    banner_font_size: f32,
    banner_bottom: f32,
    banner_opacity_min: f32,
    banner_opacity_max: f32,
}
#[derive(Clone, Deserialize)]
struct WordmarkMetrics {
    width: f32,
    height: f32,
    padding: f32,
}
pub(super) fn open(startup: Startup, cx: &mut App) -> Result<()> {
    let layout: Layout = serde_json::from_str(include_str!("../../settings/welcome-layout.json"))?;
    let commands = configuration::commands::definitions()?;
    let bounds = Bounds::centered(None, size(px(layout.width), px(layout.height)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(layout.width), px(layout.height))),
            is_resizable: false,
            titlebar: Some(gpui::TitlebarOptions {
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("欢迎使用 altaed");
            theme::setup_ui_font(window, cx);
            cx.new(|cx| Welcome::new(startup, layout, commands, window, cx))
        },
    )?;
    Ok(())
}
struct ShortcutInput {
    command: CommandDefinition,
    field: Entity<TextField>,
    _changed: Subscription,
}
struct Welcome {
    startup: Startup,
    layout: Layout,
    page: Page,
    focus: FocusHandle,
    compiler: Entity<TextField>,
    _compiler_changed: Subscription,
    shortcuts: Vec<ShortcutInput>,
    shortcut_errors: BTreeMap<String, String>,
    shortcut_message: String,
    verified: Option<PathBuf>,
    probe_cancel: Option<Arc<AtomicBool>>,
    probe_generation: u64,
    busy: bool,
    saving: bool,
    error: bool,
    message: String,
    animate: bool,
}
impl Drop for Welcome {
    fn drop(&mut self) {
        if let Some(cancel) = &self.probe_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Welcome {
    fn new(
        startup: Startup,
        layout: Layout,
        commands: Vec<CommandDefinition>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let path = startup
            .preferences
            .toolchain
            .compiler_path
            .clone()
            .or_else(runner::discover_compiler);
        let compiler = cx.new(|cx| {
            let mut field = TextField::single_line(
                path.map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                window,
                cx,
            );
            field.set_padding_xy(8., 5.);
            field.set_placeholder("clang++ / g++ 或完整路径", window, cx);
            field
        });
        let compiler_changed = cx.subscribe(&compiler, |this, _, _: &Changed, cx| {
            // Verification applies only to the exact compiler the user tested.
            this.verified = None;
            this.error = false;
            this.message = "检测将编译并运行一段 C++20 程序。".into();
            cx.notify();
        });
        let shortcuts = EXECUTION_COMMANDS
            .iter()
            .filter_map(|(id, _)| commands.iter().find(|command| command.id.0 == *id))
            .map(|command| {
                let text = command
                    .effective_shortcut(&startup.preferences.keybindings)
                    .to_owned();
                let field = cx.new(|cx| {
                    let mut field = TextField::single_line(text, window, cx);
                    field.set_padding_xy(8., 5.);
                    field.set_placeholder("未分配", window, cx);
                    field
                });
                let changed = cx.subscribe(&field, |this, _, _: &Changed, cx| {
                    this.shortcut_errors.clear();
                    this.shortcut_message.clear();
                    cx.notify();
                });
                ShortcutInput {
                    command: command.clone(),
                    field,
                    _changed: changed,
                }
            })
            .collect();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let animate =
            !startup.preferences.workbench.reduce_motion && !crate::platform::reduce_motion();
        Self {
            startup,
            layout,
            page: Page::Welcome,
            focus,
            compiler,
            _compiler_changed: compiler_changed,
            shortcuts,
            shortcut_errors: BTreeMap::new(),
            shortcut_message: String::new(),
            verified: None,
            probe_cancel: None,
            probe_generation: 0,
            busy: false,
            saving: false,
            error: false,
            message: "检测将编译并运行一段 C++20 程序。".into(),
            animate,
        }
    }
    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        self.compiler
            .update(cx, |field, cx| field.set_read_only(busy, cx));
        for input in &self.shortcuts {
            input
                .field
                .update(cx, |field, cx| field.set_read_only(busy, cx));
        }
        cx.notify();
    }
    fn show_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.page = page;
        let focus = match page {
            Page::Welcome => self.focus.clone(),
            Page::Environment => self.compiler.focus_handle(cx),
            Page::Shortcuts => self
                .shortcuts
                .first()
                .map(|input| input.field.focus_handle(cx))
                .unwrap_or_else(|| self.focus.clone()),
        };
        window.focus(&focus, cx);
        cx.notify();
    }
    fn next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        match self.page {
            Page::Welcome => self.show_page(Page::Environment, window, cx),
            Page::Environment if self.verified.is_some() => {
                self.show_page(Page::Shortcuts, window, cx)
            }
            Page::Shortcuts => self.finish(window, cx),
            _ => {}
        }
    }
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        match self.page {
            Page::Welcome => cx.quit(),
            Page::Environment => self.show_page(Page::Welcome, window, cx),
            Page::Shortcuts => self.show_page(Page::Environment, window, cx),
        }
    }
    fn skip_environment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        if let Some(cancel) = self.probe_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        // Ignore an in-flight result after skipping; it must never silently
        // replace the user's decision or persist an unconfirmed toolchain.
        self.probe_generation += 1;
        self.verified = None;
        self.error = false;
        self.message = "检测将编译并运行一段 C++20 程序。".into();
        self.set_busy(false, cx);
        self.show_page(Page::Shortcuts, window, cx);
    }
    fn detect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.verified = None;
        match runner::discover_compiler() {
            Some(path) => {
                self.compiler.update(cx, |field, cx| {
                    field.set_text(path.to_string_lossy(), window, cx)
                });
                self.error = false;
                self.message = "已找到候选编译器，请验证环境。".into();
            }
            None => {
                self.error = true;
                self.message = "未找到 C++ 编译器。请安装工具链后选择文件，或稍后配置。".into();
            }
        }
        cx.notify();
    }
    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = selection.await;
            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            this.compiler.update(cx, |field, cx| {
                                field.set_text(path.to_string_lossy(), window, cx)
                            });
                            this.verified = None;
                            this.message = "已选择编译器，请验证环境。".into();
                            this.error = false;
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        this.message = format!("无法选择编译器：{error:#}");
                        this.error = true;
                    }
                    Err(error) => {
                        this.message = format!("文件选择未完成：{error}");
                        this.error = true;
                    }
                }
                cx.notify();
            })
        })
        .detach_and_log_err(cx);
    }
    fn verify(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let path = PathBuf::from(self.compiler.read(cx).text(cx).trim());
        self.verified = None;
        self.message = "正在编译并运行检测程序…".into();
        self.error = false;
        self.probe_generation += 1;
        let generation = self.probe_generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.probe_cancel = Some(cancel.clone());
        self.set_busy(true, cx);
        let root = self.startup.directory.join("environment-checks");
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { runner::verify_compiler(&path, &root, cancel) })
                .await;
            this.update_in(cx, |this, _, cx| {
                if generation != this.probe_generation {
                    return;
                }
                this.probe_cancel = None;
                this.set_busy(false, cx);
                match result {
                    Ok(path) => {
                        this.verified = Some(path);
                        this.message = "环境已就绪 · C++20 编译和执行检测通过。".into();
                    }
                    Err(error) => {
                        this.error = true;
                        this.message = format!("检测失败：{error:#}。检查路径或工具链后重试。");
                    }
                }
                cx.notify();
            })
        })
        .detach_and_log_err(cx);
    }
    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let mut next = self.startup.preferences.clone();
        next.toolchain.compiler_path = self.verified.clone();
        next.toolchain.setup_complete = true;
        for input in &self.shortcuts {
            next.keybindings.insert(
                CommandId(input.command.id.0.clone()),
                input.field.read(cx).text(cx).trim().into(),
            );
        }
        match crate::keymap::field_errors(&next, cx) {
            Ok(errors) if errors.is_empty() => self.shortcut_errors.clear(),
            Ok(errors) => {
                self.shortcut_message = errors.values().next().cloned().unwrap_or_default();
                self.shortcut_errors = errors;
                cx.notify();
                return;
            }
            Err(error) => {
                self.shortcut_message = format!("无法验证快捷键：{error:#}");
                cx.notify();
                return;
            }
        }
        let original = self.startup.preferences.clone();
        let directory = self.startup.directory.clone();
        self.saving = true;
        self.set_busy(true, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    configuration::save(&directory, &original, &next).map(|()| next)
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.saving = false;
                this.set_busy(false, cx);
                match result {
                    Ok(next) => {
                        this.startup.preferences = next;
                        match this.startup.open_ide(cx) {
                            Ok(()) => window.remove_window(),
                            Err(error) => {
                                this.shortcut_message =
                                    format!("配置已保存，IDE 未能打开：{error:#}。点击完成重试。")
                            }
                        }
                    }
                    Err(error) => {
                        this.shortcut_message = format!("配置未保存：{error:#}。请修正后重试。")
                    }
                }
                cx.notify();
            })
        })
        .detach_and_log_err(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn every_page_renders_and_skip_cancels_verification_without_configuring_a_driver(
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
        let directory =
            std::env::temp_dir().join(format!("altaed-welcome-{}", uuid::Uuid::new_v4()));
        let mut preferences = configuration::load_or_create(&directory)
            .unwrap()
            .preferences;
        preferences.workbench.reduce_motion = true;
        let mut session = crate::session::Session::default();
        session.ensure_document();
        let language = Arc::new(language::Language::new(
            toml::from_str(include_str!("../../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let startup = Startup {
            session,
            directory: directory.clone(),
            language,
            preferences,
        };
        let (welcome, cx) = cx.add_window_view(|window, cx| {
            Welcome::new(
                startup,
                serde_json::from_str(include_str!("../../settings/welcome-layout.json")).unwrap(),
                configuration::commands::definitions().unwrap(),
                window,
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|window, cx| welcome.update(cx, |welcome, cx| welcome.next(window, cx)));
        cx.run_until_parked();
        welcome.read_with(cx, |welcome, _| {
            assert!(matches!(welcome.page, Page::Environment))
        });
        let cancel = Arc::new(AtomicBool::new(false));
        cx.update(|window, cx| {
            welcome.update(cx, |welcome, cx| {
                welcome.probe_cancel = Some(cancel.clone());
                welcome.set_busy(true, cx);
                welcome.skip_environment(window, cx);
            })
        });
        cx.run_until_parked();
        assert!(cancel.load(Ordering::Relaxed));
        welcome.read_with(cx, |welcome, _| {
            assert!(matches!(welcome.page, Page::Shortcuts));
            assert!(!welcome.busy);
            assert!(welcome.verified.is_none());
        });
        assert!(
            !configuration::load(&directory)
                .unwrap()
                .toolchain
                .setup_complete
        );
        cx.update(|window, cx| {
            welcome.update(cx, |welcome, cx| {
                welcome.shortcuts[0]
                    .field
                    .update(cx, |field, cx| field.set_text("hyper-x", window, cx));
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| welcome.update(cx, |welcome, cx| welcome.finish(window, cx)));
        cx.run_until_parked();
        welcome.read_with(cx, |welcome, _| {
            assert!(
                welcome
                    .shortcut_errors
                    .contains_key("core.keybindings.run.compile")
            )
        });
        assert!(
            !configuration::load(&directory)
                .unwrap()
                .toolchain
                .setup_complete
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}

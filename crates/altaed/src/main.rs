// Both development and release GUI binaries start without a companion console.
// Keep the test harness on the console subsystem so its output remains visible.
#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]

mod app;
mod app_assets;
mod color_theme;
mod configuration;
mod editor_adapter;
mod keymap;
mod language_server;
mod panel_style;
mod startup_error;
mod text_field;
mod workbench_layout;
use altaed_core::{
    atomic_file, checker, companion, document, platform, runner, session, storage, toolchain,
};
use anyhow::{Context as _, Result};
use gpui::{App, actions};
use language::{Language, LanguageQueries};
use settings::SettingsStore;
use std::sync::Arc;
use theme::ActiveTheme;
actions!(altaed, [Quit]);
fn start(cx: &mut App) -> Result<()> {
    assets::Assets.load_fonts(cx)?;
    let settings = SettingsStore::new(cx, &settings::default_settings());
    cx.set_global(settings);
    theme::init(theme::LoadThemes::All(Box::new(app_assets::Assets)), cx);
    editor::init(cx);
    let directory = storage::data_directory()?;
    let loaded = configuration::load_or_create(&directory)?;
    if loaded.created {
        eprintln!(
            "已生成默认配置：{}",
            directory.join(configuration::FILE_NAME).display()
        );
    }
    color_theme::apply(&loaded.preferences.editor.theme, cx)?;
    keymap::init(&loaded.preferences, cx).with_context(|| {
        format!(
            "无法加载 {}",
            directory.join(configuration::FILE_NAME).display()
        )
    })?;
    cx.set_menus(vec![
        gpui::Menu {
            name: "altaed".into(),
            items: vec![
                gpui::MenuItem::action("设置…", app::OpenSettings),
                gpui::MenuItem::action("重新载入 config.json", app::ReloadConfiguration),
                gpui::MenuItem::action("显示 config.json", app::RevealConfiguration),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("退出 altaed", Quit),
            ],
        },
        gpui::Menu {
            name: "文件".into(),
            items: vec![
                gpui::MenuItem::action("新建", app::NewDocument),
                gpui::MenuItem::action("打开…", app::OpenDocument),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("保存", app::SaveDocument),
                gpui::MenuItem::action("另存为…", app::SaveAs),
                gpui::MenuItem::action("关闭文档", app::CloseDocument),
            ],
        },
        gpui::Menu {
            name: "编辑".into(),
            items: vec![
                gpui::MenuItem::action("撤销", editor::actions::Undo),
                gpui::MenuItem::action("重做", editor::actions::Redo),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("剪切", editor::actions::Cut),
                gpui::MenuItem::action("复制", editor::actions::Copy),
                gpui::MenuItem::action("粘贴", editor::actions::Paste),
                gpui::MenuItem::action("全选", editor::actions::SelectAll),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("转到定义", app::GoToDefinition),
                gpui::MenuItem::action("重新启动语言服务", app::RestartLanguageServer),
            ],
        },
        gpui::Menu {
            name: "运行".into(),
            items: vec![
                gpui::MenuItem::action("只编译", app::CompileDocument),
                gpui::MenuItem::action("运行全部测试", app::RunTests),
                gpui::MenuItem::action("运行已有程序", app::RunCompiled),
                gpui::MenuItem::action("编译并运行", app::CompileAndRun),
                gpui::MenuItem::action("取消编译／测试", app::CancelRun),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("当前文档的运行配置…", app::ConfigureRun),
                gpui::MenuItem::action("添加测试…", app::AddTest),
            ],
        },
        gpui::Menu {
            name: "视图".into(),
            items: vec![
                gpui::MenuItem::action("显示／隐藏文档侧栏", app::ToggleDocuments),
                gpui::MenuItem::action("显示／隐藏题目与测试", app::ToggleContext),
                gpui::MenuItem::action("测试数据", app::ShowTests),
                gpui::MenuItem::action("题目信息", app::ShowProblem),
                gpui::MenuItem::action("显示／隐藏输出", app::ToggleOutput),
            ],
        },
    ]);
    cx.on_action(|_: &Quit, cx| app::request_quit(cx));
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    let cpp = Arc::new(
        Language::new(
            toml::from_str(include_str!("../languages/cpp/config.toml"))?,
            Some(tree_sitter_cpp::LANGUAGE.into()),
        )
        .with_queries(LanguageQueries {
            highlights: Some(include_str!("../languages/cpp/highlights.scm").into()),
            brackets: Some(include_str!("../languages/cpp/brackets.scm").into()),
            indents: Some(include_str!("../languages/cpp/indents.scm").into()),
            outline: Some(include_str!("../languages/cpp/outline.scm").into()),
            overrides: Some(include_str!("../languages/cpp/overrides.scm").into()),
            text_objects: Some(include_str!("../languages/cpp/textobjects.scm").into()),
            ..Default::default()
        })?,
    );
    cpp.set_theme(cx.theme().syntax());
    let mut session = storage::load_session(&directory)?;
    let preferences = loaded.preferences;
    if session.documents.is_empty() {
        let document = document::CodeDocument::untitled_with_settings(
            preferences.toolchain.new_document_settings(),
        );
        session.documents.push(document);
    }
    app::launch(session, directory, cpp, preferences, cx)?;
    cx.activate(true);
    Ok(())
}

fn main() {
    gpui_platform::application()
        .with_assets(app_assets::Assets)
        .run(|cx| {
            if let Err(error) = start(cx) {
                eprintln!("altaed could not start: {error:#}");
                if cx.has_global::<SettingsStore>() {
                    startup_error::show(format!("{error:#}"), cx);
                } else {
                    cx.quit();
                }
            }
        });
}

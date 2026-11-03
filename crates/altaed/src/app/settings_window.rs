mod view;
use super::*;
use crate::{
    configuration,
    configuration::schema::{Kind, Registry, Setting, Values},
    document::DocumentSettings,
    text_field::{Changed, TextField},
};
use gpui::{AppContext, Bounds, Entity, WeakEntity, WindowBounds, WindowOptions, px, size};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Application,
    Document,
}
impl Scope {
    fn title(self) -> &'static str {
        match self {
            Self::Application => "设置",
            Self::Document => "文档运行配置",
        }
    }
    fn explanation(self) -> &'static str {
        match self {
            Self::Application => "对整个软件生效",
            Self::Document => "修改只应用到当前文档",
        }
    }
}
#[derive(Clone)]
enum Original {
    Application(configuration::AppPreferences),
    Document {
        // Applying this form must still target its original document after a
        // tab switch; the snapshot also detects competing configuration edits.
        id: DocumentId,
        settings: DocumentSettings,
    },
}
gpui::actions!(altaed_settings, [CloseSettings, SearchSettings]);
impl Ide {
    pub(super) fn open_settings(
        &mut self,
        _: &OpenSettings,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_settings(Scope::Application, cx);
    }
    pub(super) fn show_settings(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.show_settings_category(scope, None, cx);
    }
    fn show_settings_category(
        &mut self,
        scope: Scope,
        category: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let registry = match if scope == Scope::Application {
            Registry::application()
        } else {
            Registry::builtins()
        } {
            Ok(registry) => Arc::new(registry),
            Err(error) => {
                self.status = StatusMessage::General(format!("设置描述无效：{error:#}"));
                cx.notify();
                return;
            }
        };
        let original = match scope {
            Scope::Application => Original::Application(self.preferences.clone()),
            Scope::Document => {
                let document = &self.session.documents[self.session.active];
                Original::Document {
                    id: document.id,
                    settings: document.metadata.settings.clone(),
                }
            }
        };
        let parent = cx.weak_entity();
        let bounds = Bounds::centered(
            None,
            size(px(registry.layout.width), px(registry.layout.height)),
            cx,
        );
        if let Err(error) = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(700.), px(460.))),
                ..Default::default()
            },
            move |window, cx| {
                window.set_window_title(&format!("{} — altaed", scope.title()));
                theme::setup_ui_font(window, cx);
                cx.new(|cx| {
                    SettingsWindow::new(
                        SettingsWindowSeed {
                            registry,
                            original,
                            parent,
                            category,
                        },
                        window,
                        cx,
                    )
                })
            },
        ) {
            self.status = StatusMessage::General(format!("无法打开设置：{error:#}"));
            cx.notify();
        }
    }
}

struct SettingsWindowSeed {
    registry: Arc<Registry>,
    original: Original,
    parent: WeakEntity<Ide>,
    category: Option<String>,
}
struct Input {
    field: Entity<TextField>,
    _changed: Subscription,
}
struct SettingsWindow {
    registry: Arc<Registry>,
    original: Original,
    parent: WeakEntity<Ide>,
    draft: Values,
    saved: Values,
    scope: Scope,
    category: String,
    query: String,
    search: Entity<TextField>,
    _search_changed: Subscription,
    inputs: BTreeMap<String, Input>,
    errors: BTreeMap<String, String>,
    message: String,
    saving: bool,
    // Edits may continue during disk I/O. Only this submitted snapshot becomes
    // saved; newer field contents remain a dirty draft after completion.
    submitted: Option<Values>,
    focus: gpui::FocusHandle,
}
impl SettingsWindow {
    fn new(seed: SettingsWindowSeed, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let SettingsWindowSeed {
            registry,
            original,
            parent,
            category,
        } = seed;
        let scope = match &original {
            Original::Application(_) => Scope::Application,
            Original::Document { .. } => Scope::Document,
        };
        let draft = match &original {
            Original::Application(preferences) => registry.application_values(preferences),
            Original::Document { settings, .. } => registry.values(settings),
        };
        let search = cx.new(|cx| {
            let mut field = TextField::single_line(String::new(), window, cx);
            field.set_placeholder("搜索设置…", window, cx);
            field.set_padding_xy(8., 4.);
            field.use_ui_font(window, cx);
            field
        });
        let search_changed = cx.subscribe(&search, |this, field, _: &Changed, cx| {
            this.query = field.read(cx).text(cx).trim().to_lowercase();
            this.message.clear();
            cx.notify();
        });
        let category = category.unwrap_or_else(|| registry.categories[0].id.clone());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            registry,
            original,
            parent,
            saved: draft.clone(),
            draft,
            scope,
            category,
            query: String::new(),
            search,
            _search_changed: search_changed,
            inputs: BTreeMap::new(),
            errors: BTreeMap::new(),
            message: String::new(),
            saving: false,
            submitted: None,
            focus,
        }
    }
    fn values(&self) -> &Values {
        &self.draft
    }
    fn dirty(&self) -> bool {
        self.draft != self.saved
    }
    fn visible_settings(&self) -> Vec<Setting> {
        self.registry
            .settings
            .iter()
            .filter(|setting| {
                if !setting.visible(self.values()) {
                    return false;
                }
                if self.query.is_empty() {
                    return setting.category == self.category;
                }
                let category = self
                    .registry
                    .categories
                    .iter()
                    .find(|category| category.id == setting.category);
                let haystack = format!(
                    "{} {} {} {} {}",
                    setting.label,
                    setting.description,
                    setting.id,
                    category
                        .map(|category| category.label.as_str())
                        .unwrap_or_default(),
                    setting
                        .options
                        .iter()
                        .map(|option| option.label.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                haystack.to_lowercase().contains(&self.query)
            })
            .cloned()
            .collect()
    }
    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.search
            .update(cx, |search, cx| search.set_text("", window, cx));
    }
    fn set_value(&mut self, id: String, value: String, cx: &mut Context<Self>) {
        self.draft.insert(id.clone(), value);
        self.errors.remove(&id);
        self.message.clear();
        cx.notify();
    }
    fn ensure_input(
        &mut self,
        setting: &Setting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextField> {
        if let Some(input) = self.inputs.get(&setting.id) {
            return input.field.clone();
        }
        let text = self.values().get(&setting.id).cloned().unwrap_or_default();
        let field = cx.new(|cx| {
            let mut field = if setting.kind == Kind::Multiline {
                TextField::multiline(text, false, window, cx)
            } else {
                TextField::single_line(text, window, cx)
            };
            field.set_placeholder(&setting.placeholder, window, cx);
            field.set_padding_xy(8., 7.);
            field
        });
        let id = setting.id.clone();
        let changed = cx.subscribe(&field, move |this, field, _: &Changed, cx| {
            this.set_value(id.clone(), field.read(cx).text(cx), cx);
        });
        self.inputs.insert(
            setting.id.clone(),
            Input {
                field: field.clone(),
                _changed: changed,
            },
        );
        field
    }
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for setting in self.visible_settings() {
            let value = setting.default_text();
            self.draft.insert(setting.id.clone(), value.clone());
            if let Some(input) = self.inputs.get(&setting.id) {
                input
                    .field
                    .update(cx, |input, cx| input.set_text(value, window, cx));
            }
        }
        self.errors.clear();
        self.message = "默认值已填入，点击“应用”保存".into();
        cx.notify();
    }
    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || !self.dirty() {
            return;
        }
        self.errors = self.registry.errors(self.values());
        if !self.errors.is_empty() {
            if let Some(setting) = self
                .registry
                .settings
                .iter()
                .find(|setting| self.errors.contains_key(&setting.id))
                .cloned()
            {
                self.category = setting.category.clone();
                self.clear_search(window, cx);
                let field = self.ensure_input(&setting, window, cx);
                window.focus(&field.focus_handle(cx), cx);
            }
            self.message = "请修正标记的设置".into();
            cx.notify();
            return;
        }
        match self.original.clone() {
            Original::Document { id, settings } => self.apply_document(id, settings, cx),
            Original::Application(expected) => self.apply_application(expected, window, cx),
        }
    }
    fn apply_document(
        &mut self,
        id: DocumentId,
        settings: DocumentSettings,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .registry
            .apply(&settings, self.values())
            .and_then(|next| {
                self.parent
                    .update(cx, |parent, cx| -> anyhow::Result<()> {
                        let document = parent
                            .session
                            .documents
                            .iter_mut()
                            .find(|document| document.id == id)
                            .ok_or_else(|| anyhow::anyhow!("原文档已关闭，请重新打开后编辑"))?;
                        anyhow::ensure!(
                            document.metadata.settings == settings,
                            "配置已在其他窗口修改，请重新打开设置"
                        );
                        document.metadata.settings = next.clone();
                        parent.mark_changed(id);
                        parent.sync_language_service(cx);
                        cx.notify();
                        Ok(())
                    })??;
                Ok(next)
            });
        match result {
            Ok(next) => self.applied(
                Original::Document { id, settings: next },
                "已应用到当前文档",
                cx,
            ),
            Err(error) => {
                self.message = format!("未应用：{error:#}");
                cx.notify();
            }
        }
    }
    fn apply_application(
        &mut self,
        expected: configuration::AppPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = match self
            .registry
            .apply_application(&expected, self.values())
            .and_then(|next| {
                self.errors = crate::keymap::field_errors(&next, cx)?;
                if !self.errors.is_empty() {
                    anyhow::bail!("请修正标记的快捷键");
                }
                Ok(next)
            }) {
            Ok(next) => next,
            Err(error) => {
                self.message = format!("未应用：{error:#}");
                if let Some(setting) = self
                    .registry
                    .settings
                    .iter()
                    .find(|setting| self.errors.contains_key(&setting.id))
                    .cloned()
                {
                    self.category = setting.category.clone();
                    self.clear_search(window, cx);
                    let input = self.ensure_input(&setting, window, cx);
                    window.focus(&input.focus_handle(cx), cx);
                }
                cx.notify();
                return;
            }
        };
        let preparation = self
            .parent
            .update(cx, |parent, _| -> anyhow::Result<PathBuf> {
                anyhow::ensure!(!parent.preferences_saving, "另一个窗口正在保存，请稍后再试");
                anyhow::ensure!(
                    parent.preferences == expected,
                    "软件设置已在其他窗口修改，请重新打开设置"
                );
                parent.preferences_saving = true;
                Ok(parent.directory.clone())
            })
            .and_then(|result| result);
        let directory = match preparation {
            Ok(directory) => directory,
            Err(error) => {
                self.message = format!("未保存：{error:#}");
                cx.notify();
                return;
            }
        };
        self.saving = true;
        self.submitted = Some(self.values().clone());
        self.message = "正在保存…".into();
        let parent = self.parent.clone();
        let written = next.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { configuration::save(&directory, &expected, &written) },
                )
                .await;
            let _ = parent.update(cx, |parent, cx| {
                parent.preferences_saving = false;
                if result.is_ok() {
                    parent.set_preferences(next.clone(), cx);
                }
                cx.notify();
            });
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                match result {
                    Ok(()) => this.applied(Original::Application(next), "软件设置已应用", cx),
                    Err(error) => {
                        this.submitted = None;
                        this.message = format!("未保存：{error:#}");
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn applied(&mut self, next: Original, message: &str, cx: &mut Context<Self>) {
        self.original = next;
        self.saved = self.submitted.take().unwrap_or_else(|| self.draft.clone());
        self.message = message.into();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn settings_drafts_apply_to_their_document_and_reject_stale_windows(
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
        let second = CodeDocument::untitled();
        let original = first.clone();
        let language = Arc::new(language::Language::new(
            toml::from_str(include_str!("../../languages/cpp/config.toml")).unwrap(),
            None,
        ));
        let directory =
            std::env::temp_dir().join(format!("altaed-settings-form-{}", uuid::Uuid::new_v4()));
        let (ide, _) = cx.add_window_view(|window, cx| {
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
        let parent = ide.downgrade();
        let (form, form_cx) = cx.add_window_view(|window, cx| {
            SettingsWindow::new(
                SettingsWindowSeed {
                    registry: Arc::new(Registry::builtins().unwrap()),
                    original: Original::Document {
                        id: original.id,
                        settings: original.metadata.settings,
                    },
                    parent,
                    category: None,
                },
                window,
                cx,
            )
        });
        form_cx.update(|window, cx| {
            form.update(cx, |form, cx| {
                form.category = "core.compiler".into();
                let setting = form
                    .registry
                    .settings
                    .iter()
                    .find(|setting| setting.id == "core.compiler.arguments")
                    .unwrap()
                    .clone();
                let field = form.ensure_input(&setting, window, cx);
                field.update(cx, |field, cx| {
                    field.set_text("-std=c++20\n-O0\n-Wall", window, cx)
                });
            })
        });
        form_cx.run_until_parked();
        ide.read_with(form_cx, |ide, _| {
            assert_eq!(
                ide.session.documents[0].metadata.settings,
                DocumentSettings::default(),
                "editing the draft must not mutate the document"
            )
        });
        form_cx.update(|window, cx| form.update(cx, |form, cx| form.apply(window, cx)));
        ide.read_with(form_cx, |ide, _| {
            assert_eq!(
                ide.session.documents[0]
                    .metadata
                    .settings
                    .compiler_arguments,
                ["-std=c++20", "-O0", "-Wall"]
            );
            assert_eq!(
                ide.session.documents[1].metadata.settings,
                DocumentSettings::default()
            );
            assert_eq!(ide.preferences, configuration::AppPreferences::default());
        });
        form_cx.update(|window, cx| {
            ide.update(cx, |ide, _| {
                ide.session.documents[0]
                    .metadata
                    .settings
                    .execution
                    .test_timeout_ms = 1234
            });
            form.update(cx, |form, cx| {
                form.set_value("core.execution.test_timeout".into(), "9000".into(), cx);
                form.apply(window, cx);
            });
        });
        form.read_with(form_cx, |form, _| {
            assert!(form.message.contains("其他窗口"))
        });
        ide.read_with(form_cx, |ide, _| {
            assert_eq!(
                ide.session.documents[0]
                    .metadata
                    .settings
                    .execution
                    .test_timeout_ms,
                1234
            )
        });
    }
}

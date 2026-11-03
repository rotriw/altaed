mod configuration_commands;
mod execution;
mod language_intelligence;
pub(crate) mod settings_window;
mod startup;
mod test_editors;
mod welcome;
pub(crate) use startup::launch;
mod workbench;

use crate::{
    document::{CodeDocument, DocumentId},
    editor_adapter::EditorView,
    session::Session,
    storage,
    workbench_layout::WorkbenchLayout,
};
use gpui::{Context, Focusable, PathPromptOptions, Subscription, Window, actions, prelude::*};
use language::Language;
use std::{path::PathBuf, sync::Arc};
actions!(
    altaed,
    [
        NewDocument,
        OpenDocument,
        SaveDocument,
        SaveAs,
        CloseDocument,
        ToggleDocuments,
        ToggleContext,
        ToggleOutput,
        ShowTests,
        ShowProblem,
        CompileDocument,
        RunTests,
        RunCompiled,
        CompileAndRun,
        CancelRun,
        ConfigureRun,
        OpenSettings,
        GoToDefinition,
        RestartLanguageServer,
        ReloadConfiguration,
        RevealConfiguration,
        AddTest
    ]
);

enum StatusMessage {
    General(String),
    Document(DocumentId, String),
}

impl StatusMessage {
    fn text_for(&self, active: DocumentId) -> &str {
        match self {
            Self::General(text) => text,
            Self::Document(id, text) if *id == active => text,
            Self::Document(_, _) => "就绪",
        }
    }
}

pub struct Ide {
    pub(crate) preferences: crate::configuration::AppPreferences,
    preferences_pending: bool,
    preferences_loaded: bool,
    language_service:
        Option<gpui::Entity<crate::editor_adapter::language_service::LanguageService>>,
    _language_observer: Option<Subscription>,
    navigation_request: u64,
    tabs_scroll: gpui::ScrollHandle,
    tab_auto_scroll: Option<gpui::Task<()>>,
    tab_visibility: Option<(crate::document::DocumentId, usize, gpui::Pixels)>,
    _window_bounds: Subscription,
    preferences_saving: bool,
    test_editors: std::collections::HashMap<
        DocumentId,
        std::collections::BTreeMap<usize, test_editors::TestEditors>,
    >,
    runs: execution::Runs,
    layout: WorkbenchLayout,
    sidebar: workbench::sidebar::SidebarState,
    session: Session,
    views: Vec<EditorView>,
    edits: Vec<Subscription>,
    directory: PathBuf,
    checkpoints: Arc<storage::SessionStore>,
    language: Arc<Language>,
    status: StatusMessage,
    busy: bool,
    retained: Vec<CodeDocument>,
    closing: Option<DocumentId>,
    _quit: Subscription,
}
impl Ide {
    #[cfg(test)]
    pub(crate) fn document_count(&self) -> usize {
        self.session.documents.len()
    }
    pub(crate) fn set_preferences(
        &mut self,
        next: crate::configuration::AppPreferences,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = crate::keymap::apply(&next, cx) {
            self.status = StatusMessage::General(format!("快捷键未应用：{error:#}"));
            cx.notify();
            return;
        }
        if self.preferences.workbench.documents != next.workbench.documents {
            self.layout.documents_visible = next.workbench.documents;
        }
        if self.preferences.workbench.tests != next.workbench.tests {
            self.layout.context_visible = next.workbench.tests;
        }
        if self.preferences.workbench.output != next.workbench.output {
            self.layout.output_visible = next.workbench.output;
        }
        self.preferences_pending = true;
        self.preferences = next;
        self.preferences_loaded = true;
        self.sync_language_service(cx);
        cx.notify();
    }
    pub fn new(
        mut session: Session,
        directory: PathBuf,
        language: Arc<Language>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        session.ensure_document();
        let views: Vec<_> = session
            .documents
            .iter()
            .map(|d| EditorView::new(d.text.clone(), language.clone(), window, cx))
            .collect();
        let edits = views
            .iter()
            .zip(&session.documents)
            .map(|(view, document)| {
                let id = document.id;
                cx.subscribe(&view.buffer, move |this, buffer, event, cx| {
                    if matches!(event, language::BufferEvent::Edited) {
                        if let Some(doc) =
                            this.session.documents.iter_mut().find(|doc| doc.id == id)
                        {
                            doc.text = buffer.read(cx).text();
                        }
                        this.mark_changed(id);
                        cx.notify();
                    }
                })
            })
            .collect();
        window.focus(&views[session.active].editor.focus_handle(cx), cx);
        let checkpoints = Arc::new(storage::SessionStore::new(directory.clone()));
        let window_bounds = cx.observe_window_bounds(window, |_, _, cx| cx.notify());
        let quit = cx.on_app_quit(|this, cx| {
            for run in this.runs.values() {
                run.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if let Some(service) = &this.language_service {
                service.update(cx, |service, cx| service.shutdown(cx));
            }
            let mut session = this.snapshot(cx);
            session.documents.extend(this.retained.clone());
            let store = this.checkpoints.clone();
            let revision = store.reserve();
            cx.background_spawn(async move {
                if let Err(error) = store.save(revision, &session) {
                    eprintln!("草稿保存失败: {error:#}");
                }
            })
        });
        Self {
            sidebar: Default::default(),
            preferences: Default::default(),
            preferences_pending: true,
            preferences_loaded: false,
            language_service: None,
            _language_observer: None,
            navigation_request: 0,
            tabs_scroll: gpui::ScrollHandle::new(),
            tab_auto_scroll: None,
            tab_visibility: None,
            _window_bounds: window_bounds,
            preferences_saving: false,
            test_editors: Default::default(),
            runs: Default::default(),
            layout: WorkbenchLayout::default(),
            session,
            views,
            edits,
            directory,
            checkpoints,
            language,
            status: StatusMessage::General("就绪".into()),
            busy: false,
            retained: Vec::new(),
            closing: None,
            _quit: quit,
        }
    }
    fn snapshot(&self, cx: &Context<Self>) -> Session {
        let mut snapshot = self.session.clone();
        for (document, view) in snapshot.documents.iter_mut().zip(&self.views) {
            document.text = view.text(cx);
        }
        snapshot
    }
    fn activate_document(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.session.active = index;
        self.tabs_scroll.scroll_to_item(index);
        window.focus(&self.views[index].editor.focus_handle(cx), cx);
        cx.notify();
    }
    fn append(&mut self, mut document: CodeDocument, window: &mut Window, cx: &mut Context<Self>) {
        let used = self
            .session
            .documents
            .iter()
            .chain(&self.retained)
            .filter_map(CodeDocument::untitled_number)
            .collect();
        document.ensure_untitled_number(&used, self.session.documents.len() as u64 + 1);
        let view = EditorView::new(document.text.clone(), self.language.clone(), window, cx);
        view.apply_preferences(&self.preferences.editor, cx);
        window.focus(&view.editor.focus_handle(cx), cx);
        self.session.active = self.views.len();
        let id = document.id;
        self.edits
            .push(cx.subscribe(&view.buffer, move |this, buffer, event, cx| {
                if matches!(event, language::BufferEvent::Edited) {
                    if let Some(doc) = this.session.documents.iter_mut().find(|doc| doc.id == id) {
                        doc.text = buffer.read(cx).text();
                    }
                    this.mark_changed(id);
                    cx.notify();
                }
            }));
        self.views.push(view);
        self.session.documents.push(document);
        self.sync_language_service(cx);
        cx.notify();
    }
    fn new_document(&mut self, _: &NewDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.append(CodeDocument::untitled(), window, cx);
    }
    fn open_document(&mut self, _: &OpenDocument, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let Some(paths) = paths.await?? else {
                    return anyhow::Ok(None);
                };
                let Some(path) = paths.into_iter().next() else {
                    return Ok(None);
                };
                cx.background_spawn(async move { storage::open_document(&path).map(Some) })
                    .await
            }
            .await;
            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Some(doc)) => {
                        if let Some(index) = this
                            .session
                            .documents
                            .iter()
                            .position(|d| d.path == doc.path)
                        {
                            this.session.active = index;
                            window.focus(&this.views[index].editor.focus_handle(cx), cx);
                        } else {
                            this.append(doc, window, cx);
                        }
                    }
                    Ok(None) => (),
                    Err(error) => {
                        this.status = StatusMessage::General(format!("打开失败：{error:#}"))
                    }
                }
                cx.notify();
            })
        })
        .detach_and_log_err(cx);
    }
    fn close_document_tab(&mut self, id: DocumentId, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(index) = self
            .session
            .documents
            .iter()
            .position(|document| document.id == id)
        else {
            return;
        };
        let document = &self.session.documents[index];
        if !document.is_dirty() && document.path.is_some() {
            self.remove_document(index, window, cx);
        } else {
            self.activate_document(index, window, cx);
            self.close_document(&CloseDocument, window, cx);
        }
    }
    fn close_document(&mut self, _: &CloseDocument, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let index = self.session.active;
        let document = self.snapshot(cx).documents[index].clone();
        if !document.is_dirty() && document.path.is_some() {
            self.remove_document(index, window, cx);
            return;
        }
        let answer = window.prompt(
            gpui::PromptLevel::Info,
            &format!("关闭 {}", document.title()),
            Some("保留后可从文档侧栏立即继续编辑，也会在下次启动时恢复。"),
            &["保存", "保留草稿", "丢弃", "取消"],
            cx,
        );
        self.busy = true;
        cx.spawn_in(window, async move |this, cx| {
            let choice = answer.await?;
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match choice {
                    0 => {
                        this.session.active = index;
                        this.closing = Some(document.id);
                        this.save_current(false, window, cx);
                    }
                    1 => {
                        this.retained.push(document);
                        this.remove_document(index, window, cx);
                    }
                    2 => this.remove_document(index, window, cx),
                    _ => (),
                }
            })
        })
        .detach_and_log_err(cx);
    }
    fn remove_document(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.session.active;
        self.test_editors.remove(&self.session.documents[index].id);
        self.views.remove(index);
        drop(self.edits.remove(index));
        self.session.documents.remove(index);
        self.sync_language_service(cx);
        if self.views.is_empty() {
            self.append(CodeDocument::untitled(), window, cx);
        } else {
            // Closing an inactive tab must not replace the document being edited.
            self.session.active = if active == index {
                index.min(self.views.len() - 1)
            } else if index < active {
                active - 1
            } else {
                active
            };
            window.focus(&self.views[self.session.active].editor.focus_handle(cx), cx);
        }
        cx.notify();
    }
    fn save(&mut self, _: &SaveDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.save_current(false, window, cx);
    }
    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        self.save_current(true, window, cx);
    }
    fn save_current(&mut self, save_as: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let mut session = self.snapshot(cx);
        session.documents.extend(self.retained.clone());
        let index = session.active;
        let mut document = session.documents[index].clone();
        let picker = if save_as || document.path.is_none() {
            let directory = document
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.to_owned())
                .unwrap_or_else(|| self.directory.clone());
            Some(
                cx.prompt_for_new_path(
                    &directory,
                    document
                        .path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str()),
                ),
            )
        } else {
            None
        };
        let store = self.checkpoints.clone();
        let revision = store.reserve();
        self.busy = true;
        let saving_id = document.id;
        self.status = StatusMessage::Document(saving_id, "正在保存…".into());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                if let Some(picker) = picker {
                    let Some(path) = picker.await?? else {
                        return anyhow::Ok(None);
                    };
                    if save_as && document.path.as_ref() != Some(&path) {
                        document = document.fork(path);
                    } else {
                        document.path = Some(path);
                    }
                }
                cx.background_spawn(async move {
                    store.save(revision, &session)?;
                    storage::save_document(&mut document)?;
                    anyhow::Ok(Some(document))
                })
                .await
            }
            .await;
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(Some(mut document)) => {
                        let saved_id = document.id;
                        if document.id != this.session.documents[index].id {
                            this.append(document, window, cx);
                        } else {
                            document.text = this.views[index].text(cx);
                            document.metadata = this.session.documents[index].metadata.clone();
                            this.session.documents[index] = document;
                        }
                        this.sync_language_service(cx);
                        this.status = StatusMessage::Document(saved_id, "保存完成".into());
                        if this.closing == Some(this.session.documents[index].id)
                            && !this.snapshot(cx).documents[index].is_dirty()
                        {
                            this.remove_document(index, window, cx);
                        }
                    }
                    Ok(None) => {
                        this.status = StatusMessage::Document(saving_id, "已取消保存".into())
                    }
                    Err(error) => {
                        this.status = StatusMessage::Document(
                            saving_id,
                            format!("保存失败：{error:#}；内容仍在编辑区，请重试"),
                        )
                    }
                }
                this.closing = None;
                cx.notify();
            })
        })
        .detach_and_log_err(cx);
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn save_status_never_describes_another_document() {
        let saved = CodeDocument::untitled();
        let unsaved = CodeDocument::untitled();
        for message in ["正在保存…", "保存完成", "保存失败，请重试"] {
            let status = StatusMessage::Document(saved.id, message.into());
            assert_eq!(status.text_for(saved.id), message);
            assert_eq!(status.text_for(unsaved.id), "就绪");
        }
    }
}

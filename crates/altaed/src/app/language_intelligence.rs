use super::*;
use crate::editor_adapter::language_service::{Binding, DefinitionTarget, LanguageService};

impl Ide {
    pub(super) fn sync_language_service(&mut self, cx: &mut Context<Self>) {
        if !self.preferences_loaded {
            return;
        }
        if self.language_service.is_none() {
            if !self.preferences.language_server.enabled {
                return;
            }
            let service = LanguageService::new(self.directory.join("language-service"), cx);
            self._language_observer = Some(cx.observe(&service, |_, _, cx| cx.notify()));
            self.language_service = Some(service);
        }
        let Some(service) = &self.language_service else {
            return;
        };
        let bindings = self
            .session
            .documents
            .iter()
            .zip(&self.views)
            .map(|(document, view)| Binding {
                document: document.clone(),
                buffer: view.buffer.clone(),
                editor: view.editor.clone(),
            })
            .collect();
        service.update(cx, |service, cx| {
            service.synchronize(
                bindings,
                self.preferences.language_server.clone(),
                self.preferences.editor.inlay_hints.clone(),
                self.preferences.toolchain.compiler_path.clone(),
                cx,
            )
        });
    }
    pub(super) fn restart_language_server(
        &mut self,
        _: &RestartLanguageServer,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(service) = &self.language_service {
            service.update(cx, |service, cx| service.restart(cx));
        }
    }
    pub(super) fn go_to_definition(
        &mut self,
        _: &GoToDefinition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.session.documents[self.session.active].id;
        let view = &self.views[self.session.active];
        let offset = view.cursor_offset(cx);
        let snapshot = view.buffer.read(cx).snapshot();
        let anchor = snapshot.anchor_before(offset);
        let Some(service) = &self.language_service else {
            self.status = StatusMessage::Document(id, "语言服务尚未启动".into());
            cx.notify();
            return;
        };
        let request = service.update(cx, |service, cx| service.definitions(id, anchor, cx));
        self.navigation_request += 1;
        let navigation_request = self.navigation_request;
        cx.spawn_in(window, async move |this, cx| {
            let targets = request.await;
            let target = this.update(cx, |this, cx| {
                if this.navigation_request != navigation_request
                    || this.session.documents[this.session.active].id != id
                    || this.views[this.session.active].cursor_offset(cx) != offset
                    || this.views[this.session.active]
                        .buffer
                        .read(cx)
                        .text_snapshot()
                        .version()
                        != snapshot.version()
                {
                    return None;
                }
                match targets {
                    Ok(targets) => {
                        if targets.is_empty() {
                            this.status = StatusMessage::Document(id, "没有找到定义".into());
                            cx.notify();
                        }
                        targets.into_iter().next()
                    }
                    Err(error) => {
                        this.status =
                            StatusMessage::Document(id, format!("定义跳转未完成：{error:#}"));
                        cx.notify();
                        None
                    }
                }
            })?;
            let Some(target) = target else {
                return anyhow::Ok(());
            };
            let existing = this.update_in(cx, |this, window, cx| {
                this.select_existing_definition(&target, window, cx)
            })?;
            if existing {
                return Ok(());
            }
            if target.document_id.is_some() {
                return Ok(());
            }
            let path = target.path.clone();
            let opened = cx
                .background_spawn(async move { storage::open_document(&path) })
                .await;
            this.update_in(cx, |this, window, cx| {
                if this.navigation_request != navigation_request
                    || this.session.documents[this.session.active].id != id
                    || this.views[this.session.active].cursor_offset(cx) != offset
                    || this.views[this.session.active]
                        .buffer
                        .read(cx)
                        .text_snapshot()
                        .version()
                        != snapshot.version()
                {
                    return;
                }
                match opened {
                    Ok(document) => {
                        if !this.select_existing_definition(&target, window, cx) {
                            this.append(document, window, cx);
                            this.views[this.session.active].select_range(target.range, window, cx);
                        }
                    }
                    Err(error) => {
                        this.status =
                            StatusMessage::Document(id, format!("定义文件未打开：{error:#}"))
                    }
                }
                cx.notify();
            })?;
            Ok(())
        })
        .detach_and_log_err(cx);
    }
    fn select_existing_definition(
        &mut self,
        target: &DefinitionTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let index = self.session.documents.iter().position(|document| {
            target.document_id == Some(document.id)
                || (target.document_id.is_none() && document.path.as_ref() == Some(&target.path))
        });
        let Some(index) = index else {
            return false;
        };
        self.session.active = index;
        self.views[index].select_range(target.range.clone(), window, cx);
        window.focus(&self.views[index].editor.focus_handle(cx), cx);
        cx.notify();
        true
    }
}

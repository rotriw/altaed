//! Coordinate the durable core inbox with editor views; no HTTP or file parsing
//! occurs in a window callback. Receipts are removed only after checkpointing.
use super::*;
use crate::{companion, document::DocumentSettings};
use gpui::Task;
use std::{sync::mpsc::Receiver, time::Duration};

#[derive(Default)]
pub(super) struct ImportState {
    pub active: bool,
    pub server: Option<companion::CompanionServer>,
    configuration: Option<(companion::CompanionSettings, DocumentSettings)>,
    generation: u64,
    start: Option<Task<()>>,
    receiving: Option<Task<()>>,
    pub status: String,
}
impl Ide {
    pub(super) fn sync_companion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.imports.active || self.busy {
            return;
        }
        let configuration = (
            self.preferences.companion.clone(),
            self.preferences.toolchain.new_document_settings(),
        );
        if self.imports.configuration.as_ref() == Some(&configuration) {
            return;
        }
        self.imports.configuration = Some(configuration.clone());
        self.imports.generation += 1;
        let generation = self.imports.generation;
        self.imports.receiving.take();
        let previous = self.imports.server.take();
        let directory = self.directory.clone();
        self.imports.status = "正在连接浏览器题目导入…".into();
        self.imports.start = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if let Some(previous) = previous {
                        previous.shutdown()?;
                    }
                    let (settings, defaults) = configuration;
                    if !settings.enabled {
                        return anyhow::Ok(None);
                    }
                    companion::CompanionServer::start(&settings, directory, defaults).map(Some)
                })
                .await;
            if let Err(error) = this.update_in(cx, |this, window, cx| {
                if this.imports.generation != generation {
                    return;
                }
                let receiver = match result {
                    Ok(Some((server, receiver))) => {
                        this.imports.server = Some(server);
                        this.imports.status =
                            "已连接 Competitive Companion；每次导入均新建文档。".into();
                        Some(receiver)
                    }
                    Ok(None) => {
                        this.imports.status = "浏览器题目导入已关闭。".into();
                        None
                    }
                    Err(error) => {
                        this.imports.status = format!("题目导入未连接：{error:#}");
                        None
                    }
                };
                this.receive_imports(receiver, generation, window, cx);
                cx.notify();
            }) {
                eprintln!("导入初始化未完成：{error}");
            }
        }));
    }
    fn receive_imports(
        &mut self,
        mut receiver: Option<Receiver<()>>,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = self.directory.clone();
        self.imports.receiving = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                let current = this.read_with(cx, |this, _| {
                    (this.imports.generation == generation, this.busy)
                });
                if !matches!(current, Ok((true, _))) {
                    break;
                }
                if matches!(current, Ok((true, true))) {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    continue;
                }
                let root = directory.clone();
                let pending = cx
                    .background_spawn(async move { companion::pending_documents(&root) })
                    .await;
                match pending {
                    Ok(pending) if !pending.is_empty() => {
                        let adopted = this.update_in(cx, |this, window, cx| {
                            if this.busy {
                                return None;
                            }
                            for import in &pending {
                                let id = import.document.id;
                                if !this
                                    .session
                                    .documents
                                    .iter()
                                    .chain(&this.retained)
                                    .any(|document| document.id == id)
                                {
                                    this.append(import.document.clone(), window, cx);
                                }
                            }
                            // Block close/discard until the inbox and session agree.
                            this.busy = true;
                            let mut snapshot = this.snapshot(cx);
                            snapshot.documents.extend(this.retained.clone());
                            let store = this.checkpoints.clone();
                            let revision = store.reserve();
                            cx.notify();
                            Some((snapshot, store, revision))
                        });
                        match adopted {
                            Ok(Some((snapshot, store, revision))) => {
                                let root = directory.clone();
                                let result = cx
                                    .background_spawn(async move {
                                        store.save(revision, &snapshot)?;
                                        for import in &pending {
                                            companion::acknowledge(&root, import)?;
                                        }
                                        anyhow::Ok(())
                                    })
                                    .await;
                                if this
                                    .update_in(cx, |this, _, cx| {
                                        this.busy = false;
                                        match result {
                                            Ok(()) => {
                                                this.status = StatusMessage::General(
                                                    "题目与样例已导入并保存草稿".into(),
                                                )
                                            }
                                            Err(error) => {
                                                this.status = StatusMessage::General(format!(
                                                    "导入保存未完成，将保留收据重试：{error:#}"
                                                ))
                                            }
                                        }
                                        cx.notify();
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Ok(None) => (),
                            Err(_) => break,
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(200))
                            .await;
                        continue;
                    }
                    Ok(_) => (),
                    Err(error) => {
                        if this
                            .update(cx, |this, cx| {
                                this.imports.status = format!("题目导入恢复失败：{error:#}");
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                let Some(incoming) = receiver.take() else {
                    break;
                };
                let (incoming, notification) = cx
                    .background_spawn(async move {
                        let notification = incoming.recv_timeout(Duration::from_millis(500));
                        (incoming, notification)
                    })
                    .await;
                receiver = Some(incoming);
                if matches!(
                    notification,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
                ) {
                    break;
                }
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn inbox_recovery_adopts_new_documents_and_recognizes_checkpointed_ids(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
        });
        for already_checkpointed in [false, true] {
            let root =
                std::env::temp_dir().join(format!("altaed-ui-import-{}", uuid::Uuid::new_v4()));
            let payload = serde_json::to_vec(&serde_json::json!({
                "name":"Import recovery fixture", "group":"tests", "url":"https://example.com/problem/1",
                "timeLimit":1000, "memoryLimit":128, "testType":"single",
                "input":{"type":"stdin"}, "output":{"type":"stdout"},
                "tests":[{"input":"1\n", "output":"2\n"}]
            })).unwrap();
            let id =
                companion::receive_problem(&root, &payload, DocumentSettings::default()).unwrap();
            let pending = companion::pending_documents(&root).unwrap();
            let mut session = Session::default();
            if already_checkpointed {
                session.documents.push(pending[0].document.clone());
            } else {
                session.ensure_document();
            }
            let language = Arc::new(Language::new(
                toml::from_str(include_str!("../../languages/cpp/config.toml")).unwrap(),
                None,
            ));
            let (ide, cx) = cx.add_window_view(|window, cx| {
                Ide::new(session, root.clone(), language, window, cx)
            });
            cx.update(|window, cx| {
                ide.update(cx, |ide, cx| ide.receive_imports(None, 0, window, cx))
            });
            cx.run_until_parked();
            ide.read_with(cx, |ide, _| {
                assert_eq!(
                    ide.session
                        .documents
                        .iter()
                        .filter(|document| document.id == id)
                        .count(),
                    1
                );
                assert_eq!(
                    ide.session.documents.len(),
                    if already_checkpointed { 1 } else { 2 }
                );
                assert!(!ide.busy);
            });
            assert!(companion::pending_documents(&root).unwrap().is_empty());
            let saved = storage::load_session(&root).unwrap();
            assert_eq!(
                saved
                    .documents
                    .iter()
                    .filter(|document| document.id == id)
                    .count(),
                1
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}

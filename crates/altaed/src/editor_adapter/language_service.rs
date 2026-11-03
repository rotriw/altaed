//! Standalone clangd adapter. Reuse Zed's protocol transport and editor model,
//! without constructing its workspace/project, account, Node, or collaboration
//! stores. One process serves privately identified code documents.
mod completion;
mod documents;
mod navigation;
mod process;
mod signature;
#[cfg(test)]
mod tests;
use crate::{
    document::{CodeDocument, DocumentId},
    language_server::{LanguageServerSettings, clangd_config::DocumentPlan},
    toolchain,
};
use anyhow::{Context as _, Result};
use gpui::{App, AppContext, Context, Entity, Subscription, Task};
use language::{Buffer, BufferEvent, TextBufferSnapshot};
use lsp::{LanguageServer, LanguageServerBinary, LanguageServerId, LanguageServerName};
pub(crate) use navigation::DefinitionTarget;
use parking_lot::Mutex;
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

static NEXT_SERVER_ID: AtomicUsize = AtomicUsize::new(1);
struct ClangdProvider {
    service: gpui::WeakEntity<LanguageService>,
    document_id: DocumentId,
}
#[derive(Clone)]
pub(crate) struct Binding {
    pub document: CodeDocument,
    pub buffer: Entity<Buffer>,
    pub editor: Entity<editor::Editor>,
}
struct DocumentState {
    plan: DocumentPlan,
    uri: lsp::Uri,
    buffer: Entity<Buffer>,
    version: i32,
    sent_version: i32,
    sent_snapshot: TextBufferSnapshot,
    _edits: Subscription,
    pending_change: Option<Task<()>>,
}
struct Runtime {
    server: Arc<LanguageServer>,
    _diagnostics: lsp::Subscription,
}
#[derive(Clone)]
pub(crate) enum Phase {
    Disabled,
    Preparing,
    Starting,
    Ready,
    Failed(String),
}
pub(crate) struct LanguageService {
    root: PathBuf,
    settings: LanguageServerSettings,
    drivers: BTreeSet<String>,
    documents: HashMap<DocumentId, DocumentState>,
    // Keep the last version across close/reopen so a delayed reply for the same
    // document identity cannot match a freshly attached buffer's version.
    closed_versions: HashMap<DocumentId, i32>,
    document_errors: HashMap<DocumentId, String>,
    runtime: Option<Runtime>,
    phase: Phase,
    epoch: u64,
    configuration_generation: u64,
    preparing: bool,
    preparing_task: Option<Task<()>>,
    startup_task: Option<Task<()>>,
}
impl LanguageService {
    pub fn new(root: PathBuf, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            root,
            settings: LanguageServerSettings::default(),
            drivers: BTreeSet::new(),
            documents: HashMap::new(),
            closed_versions: HashMap::new(),
            document_errors: HashMap::new(),
            runtime: None,
            phase: Phase::Disabled,
            epoch: 0,
            configuration_generation: 0,
            preparing: false,
            preparing_task: None,
            startup_task: None,
        })
    }
    pub fn status(&self, id: DocumentId) -> (&str, Option<&str>) {
        if let Some(error) = self.document_errors.get(&id) {
            return ("clangd 配置错误", Some(error));
        }
        match &self.phase {
            Phase::Disabled => ("语言服务已关闭", None),
            Phase::Preparing => ("clangd 准备中…", None),
            Phase::Starting => ("clangd 启动中…", None),
            Phase::Ready => ("clangd", None),
            Phase::Failed(error) => ("clangd 不可用", Some(error)),
        }
    }
    pub fn synchronize(
        &mut self,
        bindings: Vec<Binding>,
        settings: LanguageServerSettings,
        default_compiler: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.configuration_generation += 1;
        if !settings.enabled {
            self.preparing_task = None;
            self.preparing = false;
            self.settings = settings;
            self.stop(cx);
            self.documents.clear();
            self.document_errors.clear();
            self.phase = Phase::Disabled;
            for binding in bindings {
                binding.editor.update(cx, |editor, _| {
                    editor.set_completion_provider(None);
                    editor.set_signature_help_provider(None);
                });
            }
            cx.notify();
            return;
        }
        let generation = self.configuration_generation;
        self.preparing = true;
        let root = self.root.clone();
        if self.runtime.is_none() {
            self.phase = Phase::Preparing;
        }
        self.preparing_task = Some(cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_spawn(async move {
                    let root = if root.is_absolute() {
                        root
                    } else {
                        std::env::current_dir()?.join(root)
                    };
                    std::fs::create_dir_all(&root).context("无法创建语言服务工作目录")?;
                    let plans = bindings
                        .into_iter()
                        .map(|binding| {
                            let plan = DocumentPlan::build(
                                &binding.document,
                                &root,
                                default_compiler.as_deref(),
                            );
                            (binding, plan)
                        })
                        .collect::<Vec<_>>();
                    anyhow::Ok((root, plans))
                })
                .await;
            // Closing the IDE can release the adapter while preparation runs.
            let _ = this.update(cx, |this, cx| {
                if generation != this.configuration_generation {
                    return;
                }
                this.preparing = false;
                match prepared {
                    Ok((root, plans)) => {
                        this.root = root;
                        this.commit_bindings(plans, settings, cx);
                    }
                    Err(error) => this.fail(format!("{error:#}"), cx),
                }
            });
        }));
        cx.notify();
    }
    fn commit_bindings(
        &mut self,
        prepared: Vec<(Binding, Result<DocumentPlan>)>,
        settings: LanguageServerSettings,
        cx: &mut Context<Self>,
    ) {
        let changed = match self.reconcile_bindings(prepared, cx) {
            Ok(changed) => changed,
            Err(error) => {
                self.fail(format!("绑定语言服务文档失败：{error:#}"), cx);
                return;
            }
        };
        let drivers = self
            .documents
            .values()
            .filter_map(|document| document.plan.command.compilation_command.first())
            .filter(|driver| std::path::Path::new(driver.as_str()).is_absolute())
            .cloned()
            .collect();
        let restart =
            self.settings != settings || self.drivers != drivers || self.runtime.is_none();
        self.settings = settings;
        self.drivers = drivers;
        if !self.settings.enabled {
            self.stop(cx);
            self.phase = Phase::Disabled;
        } else if restart {
            self.restart(cx);
        } else if let Err(error) = self.reconfigure_documents(changed, cx) {
            self.fail(format!("更新语言服务配置失败：{error:#}"), cx);
        }

        cx.notify();
    }
    /// Reconcile identity/buffer ownership separately from restarting a process.
    fn reconcile_bindings(
        &mut self,
        prepared: Vec<(Binding, Result<DocumentPlan>)>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<DocumentId>> {
        let mut incoming = HashMap::new();
        self.document_errors.clear();
        for (binding, plan) in prepared {
            match plan.and_then(|plan| Ok((plan.uri()?, plan))) {
                Ok((uri, plan)) => {
                    incoming.insert(plan.id, (binding, uri, plan));
                }
                Err(error) => {
                    binding
                        .editor
                        .update(cx, |editor, _| editor.set_completion_provider(None));
                    self.document_errors
                        .insert(binding.document.id, format!("{error:#}"));
                }
            }
        }
        let removed = self
            .documents
            .keys()
            .filter(|id| !incoming.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        for id in removed {
            self.close_document(id, cx);
        }
        let mut changed = Vec::new();
        for (id, (binding, uri, plan)) in incoming {
            if self.attach_binding(binding, uri, plan, cx)? {
                changed.push(id);
            }
        }
        Ok(changed)
    }
    fn attach_binding(
        &mut self,
        binding: Binding,
        uri: lsp::Uri,
        plan: DocumentPlan,
        cx: &mut Context<Self>,
    ) -> Result<bool> {
        let id = plan.id;
        let replace = self
            .documents
            .get(&id)
            .is_none_or(|old| old.buffer.entity_id() != binding.buffer.entity_id());
        let changed = if replace {
            self.close_document(id, cx);
            let version = self
                .closed_versions
                .get(&id)
                .map_or(Some(0), |version| version.checked_add(1))
                .context("语言服务文档版本已耗尽，请重启软件")?;
            let edits = cx.subscribe(&binding.buffer, move |this, _, event, cx| {
                if matches!(event, BufferEvent::Edited) {
                    this.buffer_edited(id, cx);
                }
            });
            self.documents.insert(
                id,
                DocumentState {
                    plan,
                    uri,
                    buffer: binding.buffer.clone(),
                    version,
                    sent_version: -1,
                    sent_snapshot: binding.buffer.read(cx).text_snapshot(),
                    _edits: edits,
                    pending_change: None,
                },
            );
            true
        } else if let Some(old) = self.documents.get_mut(&id)
            && old.plan != plan
        {
            old.plan = plan;
            old.version = old
                .version
                .checked_add(1)
                .context("语言服务文档版本已耗尽，请重启软件")?;
            true
        } else {
            false
        };
        let provider = Rc::new(ClangdProvider {
            service: cx.weak_entity(),
            document_id: id,
        });
        binding.editor.update(cx, |editor, _| {
            editor.set_completion_provider(Some(provider.clone()));
            editor.set_signature_help_provider(Some(provider));
        });
        Ok(changed)
    }
    fn reconfigure_documents(
        &mut self,
        changed: Vec<DocumentId>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let Some(runtime) = &self.runtime else {
            return Ok(());
        };
        let server = runtime.server.clone();
        self.send_compilation_commands(&server)?;
        for id in changed {
            self.clear_diagnostics(id, cx);
            if let Some(document) = self.documents.get(&id)
                && document.sent_version >= 0
            {
                server.notify::<lsp::notification::DidCloseTextDocument>(
                    lsp::DidCloseTextDocumentParams {
                        text_document: lsp::TextDocumentIdentifier {
                            uri: document.uri.clone(),
                        },
                    },
                )?;
            }
            self.open_document(id, &server, cx)?;
        }
        Ok(())
    }
    fn send_compilation_commands(&self, server: &LanguageServer) -> Result<()> {
        let mut commands = serde_json::Map::new();
        for document in self.documents.values() {
            let path = document
                .plan
                .source
                .to_str()
                .context("语言服务路径不是 UTF-8")?;
            commands.insert(path.into(), serde_json::to_value(&document.plan.command)?);
        }
        server.notify::<lsp::notification::DidChangeConfiguration>(
            lsp::DidChangeConfigurationParams {
                settings: serde_json::json!({"compilationDatabaseChanges":commands}),
            },
        )
    }
}

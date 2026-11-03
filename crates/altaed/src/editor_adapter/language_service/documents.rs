use super::*;
use language::{
    Diagnostic, DiagnosticEntry, DiagnosticSet, PointUtf16, point_from_lsp, point_to_lsp,
};
const CHANGE_DEBOUNCE: Duration = Duration::from_millis(120);

impl LanguageService {
    pub(super) fn clear_diagnostics(&self, id: DocumentId, cx: &mut Context<Self>) {
        if let (Some(runtime), Some(document)) = (&self.runtime, self.documents.get(&id)) {
            let server_id = runtime.server.server_id();
            document.buffer.update(cx, |buffer, cx| {
                buffer.update_diagnostics(
                    server_id,
                    DiagnosticSet::new(
                        Vec::<DiagnosticEntry<PointUtf16>>::new(),
                        &buffer.text_snapshot(),
                    ),
                    cx,
                )
            });
        }
    }
    pub(super) fn close_document(&mut self, id: DocumentId, cx: &mut Context<Self>) {
        self.clear_inlay_hints(id, cx);
        self.clear_diagnostics(id, cx);
        if let (Some(runtime), Some(document)) = (&self.runtime, self.documents.get(&id)) {
            document.buffer.update(cx, |buffer, cx| {
                buffer.set_completion_triggers(runtime.server.server_id(), BTreeSet::new(), cx)
            });
        }
        let Some(document) = self.documents.remove(&id) else {
            return;
        };
        self.closed_versions.insert(id, document.version);
        if document.sent_version >= 0
            && let Some(runtime) = &self.runtime
            && let Err(error) = runtime
                .server
                .notify::<lsp::notification::DidCloseTextDocument>(
                    lsp::DidCloseTextDocumentParams {
                        text_document: lsp::TextDocumentIdentifier { uri: document.uri },
                    },
                )
        {
            self.fail(format!("关闭语言服务文档失败：{error:#}"), cx);
        }
    }
    pub(super) fn open_document(
        &mut self,
        id: DocumentId,
        server: &LanguageServer,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let document = self.documents.get_mut(&id).context("语言服务文档已关闭")?;
        let snapshot = document.buffer.read(cx).text_snapshot();
        server.notify::<lsp::notification::DidOpenTextDocument>(
            lsp::DidOpenTextDocumentParams {
                text_document: lsp::TextDocumentItem {
                    uri: document.uri.clone(),
                    language_id: "cpp".into(),
                    version: document.version,
                    text: snapshot.text(),
                },
            },
        )?;
        let triggers = server
            .capabilities()
            .completion_provider
            .and_then(|provider| provider.trigger_characters)
            .unwrap_or_default()
            .into_iter()
            .collect();
        document.buffer.update(cx, |buffer, cx| {
            buffer.set_completion_triggers(server.server_id(), triggers, cx)
        });
        document.sent_version = document.version;
        document.observed_snapshot = snapshot.clone();
        document.sent_snapshot = snapshot;
        Ok(())
    }
    pub(super) fn buffer_edited(&mut self, id: DocumentId, cx: &mut Context<Self>) {
        self.clear_inlay_hints(id, cx);
        self.clear_diagnostics(id, cx);
        if let Err(error) = self.observe_buffer_edits(id, cx) {
            self.fail(format!("同步代码版本失败：{error:#}"), cx);
            return;
        }
        let Some(document) = self.documents.get_mut(&id) else {
            return;
        };
        document.pending_change = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CHANGE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = this.flush_change(id, cx) {
                    this.fail(format!("同步代码失败：{error:#}"), cx)
                }
            });
        }));
        self.refresh_inlay_hints(id, cx);
        cx.notify();
    }
    pub(super) fn observe_buffer_edits(&mut self, id: DocumentId, cx: &App) -> Result<()> {
        let document = self.documents.get_mut(&id).context("语言服务文档已关闭")?;
        let snapshot = document.buffer.read(cx).text_snapshot();
        if snapshot.version() != document.observed_snapshot.version() {
            document.version = document
                .version
                .checked_add(1)
                .context("语言服务文档版本已耗尽，请重启软件")?;
            document.observed_snapshot = snapshot;
        }
        Ok(())
    }
    pub(super) fn flush_change(&mut self, id: DocumentId, cx: &mut Context<Self>) -> Result<()> {
        let Some(runtime) = &self.runtime else {
            return Ok(());
        };
        let server = &runtime.server;
        let Some(document) = self.documents.get_mut(&id) else {
            return Ok(());
        };
        if document.sent_version < 0 || document.sent_version == document.version {
            return Ok(());
        }
        let buffer = document.buffer.read(cx);
        let next = buffer.text_snapshot();
        let kind = server
            .capabilities()
            .text_document_sync
            .and_then(|sync| match sync {
                lsp::TextDocumentSyncCapability::Kind(kind) => Some(kind),
                lsp::TextDocumentSyncCapability::Options(options) => options.change,
            });
        let mut changes = if kind == Some(lsp::TextDocumentSyncKind::INCREMENTAL) {
            buffer
                .edits_since::<PointUtf16>(document.sent_snapshot.version())
                .map(|edit| {
                    let start = edit.new.start;
                    let end = start + (edit.old.end - edit.old.start);
                    lsp::TextDocumentContentChangeEvent {
                        range: Some(lsp::Range::new(point_to_lsp(start), point_to_lsp(end))),
                        range_length: None,
                        text: next.text_for_range(edit.new).collect(),
                    }
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        // The full update is also needed when multiple edits cancel out: the
        // content can match while the protocol version still must advance.
        if changes.is_empty() {
            changes.push(lsp::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: next.text(),
            });
        }
        server.notify::<lsp::notification::DidChangeTextDocument>(
            lsp::DidChangeTextDocumentParams {
                text_document: lsp::VersionedTextDocumentIdentifier {
                    uri: document.uri.clone(),
                    version: document.version,
                },
                content_changes: changes,
            },
        )?;
        document.sent_version = document.version;
        document.sent_snapshot = next;
        Ok(())
    }
    pub(super) fn publish_diagnostics(
        &mut self,
        epoch: u64,
        params: lsp::PublishDiagnosticsParams,
        cx: &mut Context<Self>,
    ) {
        if epoch != self.epoch || self.preparing {
            return;
        }
        let Some(runtime) = &self.runtime else {
            return;
        };
        let server_id = runtime.server.server_id();
        let Some((id, document)) = self
            .documents
            .iter()
            .find(|(_, document)| document.uri == params.uri)
        else {
            return;
        };
        let snapshot = document.buffer.read(cx).text_snapshot();
        if document.version != document.sent_version
            || snapshot.version() != document.sent_snapshot.version()
        {
            return;
        }
        // clangd returns versions. An unversioned initial response is safe only
        // before the first edit; never label later unknown-version results current.
        if params
            .version
            .is_some_and(|version| version != document.sent_version)
            || (params.version.is_none() && document.version != 0)
        {
            return;
        }
        let entries = params
            .diagnostics
            .into_iter()
            .enumerate()
            .filter_map(|(index, diagnostic)| {
                let start = snapshot
                    .clip_point_utf16(point_from_lsp(diagnostic.range.start), language::Bias::Left);
                let end = snapshot
                    .clip_point_utf16(point_from_lsp(diagnostic.range.end), language::Bias::Right);
                (start <= end).then_some(DiagnosticEntry {
                    range: start..end,
                    diagnostic: Diagnostic {
                        severity: diagnostic
                            .severity
                            .unwrap_or(lsp::DiagnosticSeverity::ERROR),
                        source: diagnostic.source.or_else(|| Some("clangd".into())),
                        message: diagnostic.message,
                        group_id: index,
                        is_primary: true,
                        ..Default::default()
                    },
                })
            })
            .collect::<Vec<_>>();
        let diagnostics = DiagnosticSet::new(entries, &snapshot);
        let buffer = document.buffer.clone();
        let id = *id;
        buffer.update(cx, |buffer, cx| {
            buffer.update_diagnostics(server_id, diagnostics, cx)
        });
        self.document_errors.remove(&id);
        cx.notify();
    }
}

//! LSP hints use the editor's native inlay map, never synthetic source edits.
//! Requests are debounced and tied to the same version/epoch as other providers.
use super::*;
use editor::Inlay;
use language::{ToPointUtf16, point_to_lsp};

const HINT_DEBOUNCE: Duration = Duration::from_millis(250);
const MAX_HINTS: usize = 2_000;
const MAX_LABEL_CHARACTERS: usize = 512;

#[derive(Default)]
pub(super) struct DocumentHints {
    pub editor: Option<gpui::WeakEntity<editor::Editor>>,
    ids: Vec<project::InlayId>,
    refresh: Option<Task<()>>,
    pub error: Option<String>,
}
impl LanguageService {
    pub(super) fn clear_inlay_hints(&mut self, id: DocumentId, cx: &mut Context<Self>) {
        if let Some(document) = self.documents.get_mut(&id) {
            let hints = &mut document.hints;
            hints.refresh = None;
            hints.error = None;
            let ids = std::mem::take(&mut hints.ids);
            if !ids.is_empty()
                && let Some(editor) = &hints.editor
            {
                let _ = editor.update(cx, |editor, cx| editor.splice_inlays(&ids, Vec::new(), cx));
            }
        }
    }
    pub(super) fn refresh_all_inlay_hints(&mut self, cx: &mut Context<Self>) {
        for id in self.documents.keys().copied().collect::<Vec<_>>() {
            self.clear_inlay_hints(id, cx);
            self.refresh_inlay_hints(id, cx);
        }
    }
    pub(super) fn refresh_inlay_hints(&mut self, id: DocumentId, cx: &mut Context<Self>) {
        if !self.hint_settings.enabled
            || !(self.hint_settings.parameters || self.hint_settings.types)
        {
            return;
        }
        let Some(document) = self.documents.get_mut(&id) else {
            return;
        };
        if document.hints.editor.is_none() {
            return;
        }
        document.hints.refresh = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HINT_DEBOUNCE).await;
            let prepared = this.update(cx, |this, cx| this.request_context(id, cx));
            let Ok(Ok(request)) = prepared else {
                return;
            };
            if !request
                .server
                .capabilities()
                .inlay_hint_provider
                .is_some_and(|capability| !matches!(capability, lsp::OneOf::Left(false)))
            {
                let _ = this.update(cx, |this, cx| {
                    if this.request_is_current(id, &request, cx)
                        && let Some(document) = this.documents.get_mut(&id)
                    {
                        document.hints.error = Some(
                            "当前 clangd 未声明标准行内提示支持；可在语言服务设置中更换 clangd。"
                                .into(),
                        );
                        cx.notify();
                    }
                });
                return;
            }
            let end = request.snapshot.len().to_point_utf16(&request.snapshot);
            let response = request
                .server
                .request::<lsp::request::InlayHintRequest>(
                    lsp::InlayHintParams {
                        text_document: lsp::TextDocumentIdentifier {
                            uri: request.uri.clone(),
                        },
                        range: lsp::Range::new(lsp::Position::new(0, 0), point_to_lsp(end)),
                        work_done_progress_params: Default::default(),
                    },
                    request.timeout,
                )
                .await
                .into_response();
            let _ = this.update(cx, |this, cx| {
                if !this.request_is_current(id, &request, cx) {
                    return;
                }
                let hints = match response {
                    Ok(hints) => hints.unwrap_or_default(),
                    Err(error) => {
                        if let Some(document) = this.documents.get_mut(&id) {
                            document.hints.error =
                                Some(format!("行内提示请求失败：{error:#}；补全与编译仍可使用。"));
                        }
                        cx.notify();
                        return;
                    }
                };
                let settings = this.hint_settings.clone();
                let Some(document) = this.documents.get_mut(&id) else {
                    return;
                };
                let Some(editor) = &document.hints.editor else {
                    return;
                };
                let previous = std::mem::take(&mut document.hints.ids);
                if let Ok(ids) = editor.update(cx, |editor, cx| {
                    let snapshot = editor.buffer().read(cx).snapshot(cx);
                    let inlays: Vec<_> = hints
                        .into_iter()
                        .filter_map(|hint| {
                            let label = label(&hint, &settings)?;
                            let range = lsp::Range::new(hint.position, hint.position);
                            let anchor = completion::anchors(range, &request.snapshot).ok()?.start;
                            let position = snapshot.as_singleton_anchor(anchor)?;
                            let hint = project::InlayHint {
                                position: anchor,
                                label: project::InlayHintLabel::String(label),
                                kind: hint.kind.and_then(|kind| match kind {
                                    lsp::InlayHintKind::TYPE => Some(settings::InlayHintKind::Type),
                                    lsp::InlayHintKind::PARAMETER => {
                                        Some(settings::InlayHintKind::Parameter)
                                    }
                                    _ => None,
                                }),
                                padding_left: false,
                                padding_right: false,
                                tooltip: None,
                                resolve_state: project::ResolveState::Resolved,
                            };
                            Some((position, hint))
                        })
                        .take(MAX_HINTS)
                        .enumerate()
                        .map(|(index, (position, hint))| {
                            Inlay::hint(project::InlayId::Hint(index), position, &hint)
                        })
                        .collect();
                    // This standalone editor has no Project hint provider; the
                    // adapter owns the Hint ID namespace within each view.
                    let ids = inlays.iter().map(|inlay| inlay.id).collect();
                    editor.splice_inlays(&previous, inlays, cx);
                    ids
                }) {
                    document.hints.ids = ids;
                    document.hints.error = None;
                }
                cx.notify();
            });
        }));
    }
}
fn label(
    hint: &lsp::InlayHint,
    settings: &crate::language_server::InlayHintSettings,
) -> Option<String> {
    if !settings.enabled
        || !match hint.kind {
            Some(lsp::InlayHintKind::TYPE) => settings.types,
            Some(lsp::InlayHintKind::PARAMETER) => settings.parameters,
            _ => false,
        }
    {
        return None;
    }
    let mut text: String = match &hint.label {
        lsp::InlayHintLabel::String(text) => text.clone(),
        lsp::InlayHintLabel::LabelParts(parts) => {
            parts.iter().map(|part| part.value.as_str()).collect()
        }
    };
    if text.is_empty()
        || text.contains(['\n', '\r', '\t'])
        || text.chars().count() > MAX_LABEL_CHARACTERS
    {
        return None;
    }
    if hint.padding_left == Some(true) && !text.starts_with(' ') {
        text.insert(0, ' ');
    }
    if hint.padding_right == Some(true) && !text.ends_with(' ') {
        text.push(' ');
    }
    // Commands, text edits and label links are deliberately not executed.
    Some(text)
}

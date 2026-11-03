use super::*;
use editor::{CompletionContext, CompletionProvider, Editor, ExcerptId};
use gpui::Window;
use language::{
    Anchor, Bias, BufferSnapshot, CharKind, CodeLabel, ToOffset, ToPointUtf16, point_from_lsp,
    point_to_lsp,
};
use project::{
    Completion, CompletionDisplayOptions, CompletionResponse, CompletionSource,
    lsp_store::CompletionDocumentation,
};
use std::{cell::RefCell, ops::Range};

impl CompletionProvider for ClangdProvider {
    fn completions(
        &self,
        _: ExcerptId,
        _: &Entity<Buffer>,
        position: Anchor,
        trigger: CompletionContext,
        _: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<Result<Vec<CompletionResponse>>> {
        let id = self.document_id;
        let prepared = self.service.update(cx, |service, cx| {
            if service.preparing
                || !matches!(service.phase, Phase::Ready)
                || !service.documents.contains_key(&id)
            {
                return Ok(None);
            }
            match service.request_context(id, cx) {
                Ok(request) => Ok(Some(request)),
                Err(error) => {
                    service.fail(format!("补全准备失败：{error:#}"), cx);
                    Err(error)
                }
            }
        });
        let request = match prepared {
            Ok(Ok(Some(request))) => request,
            Ok(Err(error)) => return Task::ready(Err(error)),
            _ => return Task::ready(Ok(Vec::new())),
        };
        let service = self.service.clone();
        let query_position = point_to_lsp(position.to_point_utf16(&request.snapshot));
        cx.spawn(async move |_, cx| {
            let response = request
                .server
                .request::<lsp::request::Completion>(
                    lsp::CompletionParams {
                        text_document_position: lsp::TextDocumentPositionParams {
                            text_document: lsp::TextDocumentIdentifier {
                                uri: request.uri.clone(),
                            },
                            position: query_position,
                        },
                        context: Some(trigger),
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    },
                    request.timeout,
                )
                .await
                .into_response();
            service.update(cx, |service, cx| {
                if !service.request_is_current(id, &request, cx) {
                    return Ok(Vec::new());
                }
                let response = match response {
                    Ok(response) => response,
                    Err(error) => {
                        service.fail(format!("补全请求失败：{error:#}"), cx);
                        return Err(error);
                    }
                };
                let Some(response) = response else {
                    return Ok(Vec::new());
                };
                let (items, defaults, is_incomplete) = match response {
                    lsp::CompletionResponse::Array(items) => (items, None, false),
                    lsp::CompletionResponse::List(list) => (
                        list.items,
                        list.item_defaults.map(Arc::new),
                        list.is_incomplete,
                    ),
                };
                let completions = items
                    .into_iter()
                    // This increment supports one replacement/snippet. Do not
                    // offer an item whose correctness requires unimplemented
                    // workspace edits or commands (e.g. automatic includes).
                    .filter(|item| {
                        item.additional_text_edits
                            .as_ref()
                            .is_none_or(Vec::is_empty)
                            && item.command.is_none()
                    })
                    .filter_map(|item| {
                        match convert(
                            item,
                            defaults.clone(),
                            request.server.server_id(),
                            &request.snapshot,
                            position,
                        ) {
                            Ok(completion) => Some(completion),
                            Err(error) => {
                                eprintln!("忽略无效 clangd 补全项：{error:#}");
                                None
                            }
                        }
                    })
                    .collect();
                Ok(vec![CompletionResponse {
                    completions,
                    display_options: CompletionDisplayOptions::default(),
                    is_incomplete,
                }])
            })?
        })
    }
    fn is_completion_trigger(
        &self,
        buffer: &Entity<Buffer>,
        _: Anchor,
        text: &str,
        trigger_in_words: bool,
        cx: &mut Context<Editor>,
    ) -> bool {
        let mut characters = text.chars();
        let Some(character) = characters.next() else {
            return false;
        };
        characters.next().is_none()
            && ((trigger_in_words && (character.is_alphanumeric() || character == '_'))
                || buffer.read(cx).completion_triggers().contains(text))
    }

    fn resolve_completions(
        &self,
        _: Entity<Buffer>,
        indices: Vec<usize>,
        completions: Rc<RefCell<Box<[Completion]>>>,
        cx: &mut Context<Editor>,
    ) -> Task<Result<bool>> {
        let id = self.document_id;
        let request = self
            .service
            .update(cx, |service, cx| service.request_context(id, cx));
        let request = match request {
            Ok(Ok(request))
                if request
                    .server
                    .capabilities()
                    .completion_provider
                    .is_some_and(|provider| provider.resolve_provider == Some(true)) =>
            {
                request
            }
            _ => return Task::ready(Ok(false)),
        };
        // Copy protocol items before awaiting. Never hold a RefCell borrow over
        // a request: the native menu can change selection while it is in flight.
        let pending = {
            let items = completions.borrow();
            indices
                .into_iter()
                .filter_map(|index| {
                    let completion = items.get(index)?;
                    match &completion.source {
                        CompletionSource::Lsp {
                            server_id,
                            resolved: false,
                            ..
                        } if *server_id == request.server.server_id() => {
                            Some((index, completion.source.lsp_completion(true)?.into_owned()))
                        }
                        _ => None,
                    }
                })
                .collect::<Vec<_>>()
        };
        let service = self.service.clone();
        cx.spawn(async move |_, cx| {
            let mut changed = false;
            for (index, item) in pending {
                let resolved = request
                    .server
                    .request::<lsp::request::ResolveCompletionItem>(item, request.timeout)
                    .await
                    .into_response()
                    .context("clangd 补全说明请求失败")?;
                let current = service.read_with(cx, |service, cx| {
                    service.request_is_current(id, &request, cx)
                })?;
                if !current {
                    break;
                }
                let mut items = completions.borrow_mut();
                let Some(completion) = items.get_mut(index) else {
                    continue;
                };
                // Resolve is advertised only for presentation fields. Preserve
                // the already offered edits/snippet and its confirmation rules.
                if let Some(documentation) = documentation(resolved.documentation) {
                    completion.documentation = Some(documentation);
                }
                if let CompletionSource::Lsp {
                    lsp_completion,
                    resolved: done,
                    ..
                } = &mut completion.source
                {
                    if resolved.detail.is_some() {
                        lsp_completion.detail = resolved.detail;
                    }
                    if resolved.label_details.is_some() {
                        lsp_completion.label_details = resolved.label_details;
                    }
                    completion.label = code_label(lsp_completion);
                    *done = true;
                }
                changed = true;
            }
            Ok(changed)
        })
    }
}
pub(super) fn anchors(range: lsp::Range, snapshot: &BufferSnapshot) -> Result<Range<Anchor>> {
    anyhow::ensure!(range.start <= range.end, "clangd 返回了倒置的补全替换范围");
    let start = snapshot.clip_point_utf16(point_from_lsp(range.start), Bias::Left);
    let end = snapshot.clip_point_utf16(point_from_lsp(range.end), Bias::Right);
    anyhow::ensure!(
        point_to_lsp(start) == range.start && point_to_lsp(end) == range.end,
        "clangd 返回了越界或拆分 Unicode 字符的补全范围"
    );
    Ok(snapshot.anchor_before(start)..snapshot.anchor_after(end))
}
pub(super) fn convert(
    item: lsp::CompletionItem,
    defaults: Option<Arc<lsp::CompletionListItemDefaults>>,
    server_id: LanguageServerId,
    snapshot: &BufferSnapshot,
    position: Anchor,
) -> Result<Completion> {
    let item = apply_defaults(item, defaults.as_deref());
    let mut source = CompletionSource::Lsp {
        insert_range: None,
        server_id,
        lsp_completion: Box::new(item),
        lsp_defaults: None,
        resolved: false,
    };
    let item = source
        .lsp_completion(true)
        .context("补全来源不包含 LSP 数据")?
        .into_owned();
    let cursor = point_to_lsp(position.to_point_utf16(snapshot));
    let replacement = |range: lsp::Range| -> Result<Range<Anchor>> {
        anyhow::ensure!(
            range.start.line == range.end.line && range.start <= cursor && cursor <= range.end,
            "clangd 补全范围必须包含请求光标且位于同一行"
        );
        anchors(range, snapshot)
    };
    let (replace_range, new_text) = match &item.text_edit {
        Some(lsp::CompletionTextEdit::Edit(edit)) => {
            (replacement(edit.range)?, edit.new_text.clone())
        }
        Some(lsp::CompletionTextEdit::InsertAndReplace(edit)) => {
            anyhow::ensure!(
                edit.insert.start == edit.replace.start && edit.insert.end <= edit.replace.end,
                "clangd 补全的插入范围不是替换范围的前缀"
            );
            let insert = replacement(edit.insert)?;
            if let CompletionSource::Lsp { insert_range, .. } = &mut source {
                *insert_range = Some(insert);
            }
            (replacement(edit.replace)?, edit.new_text.clone())
        }
        None => {
            let offset = position.to_offset(snapshot);
            let (word, kind) = snapshot.surrounding_word(offset, None);
            let range = if kind == Some(CharKind::Word) {
                word
            } else {
                offset..offset
            };
            (
                snapshot.anchor_before(range.start)..snapshot.anchor_after(range.end),
                item.insert_text
                    .clone()
                    .unwrap_or_else(|| item.label.trim_start().to_owned()),
            )
        }
    };
    let label = code_label(&item);
    let documentation = documentation(item.documentation);
    Ok(Completion {
        match_start: Some(replace_range.start),
        replace_range,
        new_text,
        label,
        documentation,
        source,
        icon_path: None,
        snippet_deduplication_key: None,
        insert_text_mode: item.insert_text_mode,
        confirm: None,
    })
}

/// Materialize LSP list defaults once at the boundary. In particular,
/// textEditText overrides label when editRange is supplied by the list.
fn apply_defaults(
    mut item: lsp::CompletionItem,
    defaults: Option<&lsp::CompletionListItemDefaults>,
) -> lsp::CompletionItem {
    let Some(defaults) = defaults else {
        return item;
    };
    item.commit_characters = item
        .commit_characters
        .or_else(|| defaults.commit_characters.clone());
    item.insert_text_format = item.insert_text_format.or(defaults.insert_text_format);
    item.insert_text_mode = item.insert_text_mode.or(defaults.insert_text_mode);
    item.data = item.data.or_else(|| defaults.data.clone());
    if item.text_edit.is_none()
        && let Some(range) = &defaults.edit_range
    {
        let new_text = item
            .text_edit_text
            .clone()
            .unwrap_or_else(|| item.label.clone());
        item.text_edit = Some(match range {
            lsp::CompletionListItemDefaultsEditRange::Range(range) => {
                lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                    range: *range,
                    new_text,
                })
            }
            lsp::CompletionListItemDefaultsEditRange::InsertAndReplace { insert, replace } => {
                lsp::CompletionTextEdit::InsertAndReplace(lsp::InsertReplaceEdit {
                    new_text,
                    insert: *insert,
                    replace: *replace,
                })
            }
        });
    }
    item
}

fn code_label(item: &lsp::CompletionItem) -> CodeLabel {
    let name = item.label.trim_start();
    let mut text = name.to_owned();
    if let Some(details) = &item.label_details {
        if let Some(detail) = &details.detail {
            text.push_str(detail);
        }
        if let Some(description) = &details.description {
            text.push_str(&format!("  {description}"));
        }
    } else if let Some(detail) = &item.detail
        && !detail.trim().is_empty()
        && !name.contains(detail)
    {
        text.push_str(&format!("  {detail}"));
    }
    CodeLabel::filtered(
        text,
        name.len(),
        item.filter_text.as_deref().or(Some(name)),
        Vec::new(),
    )
}

fn documentation(value: Option<lsp::Documentation>) -> Option<CompletionDocumentation> {
    match value {
        Some(lsp::Documentation::String(text)) => {
            Some(CompletionDocumentation::MultiLinePlainText(text.into()))
        }
        Some(lsp::Documentation::MarkupContent(content))
            if content.kind == lsp::MarkupKind::Markdown =>
        {
            Some(CompletionDocumentation::MultiLineMarkdown(
                content.value.into(),
            ))
        }
        Some(lsp::Documentation::MarkupContent(content)) => Some(
            CompletionDocumentation::MultiLinePlainText(content.value.into()),
        ),
        None => None,
    }
}

use super::*;
use language::{Anchor, BufferSnapshot, PointUtf16, ToPointUtf16, point_to_lsp};
use std::ops::Range;

pub(crate) struct DefinitionTarget {
    pub document_id: Option<DocumentId>,
    pub path: PathBuf,
    pub range: Range<PointUtf16>,
}
pub(super) struct RequestContext {
    pub server: Arc<LanguageServer>,
    pub uri: lsp::Uri,
    pub snapshot: BufferSnapshot,
    pub buffer_id: gpui::EntityId,
    pub version: i32,
    pub epoch: u64,
    pub configuration_generation: u64,
    pub timeout: Duration,
}
impl LanguageService {
    pub(super) fn request_context(
        &mut self,
        id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<RequestContext> {
        anyhow::ensure!(
            !self.preparing && matches!(self.phase, Phase::Ready),
            "语言服务尚未就绪"
        );
        // Automatic completion runs inside the input callback, before GPUI
        // delivers BufferEvent::Edited. Observe the actual snapshot first;
        // the later event must not invalidate this same edit a second time.
        self.observe_buffer_edits(id, cx)?;
        self.flush_change(id, cx)?;
        let document = self.documents.get(&id).context("语言服务文档已关闭")?;
        let server = self
            .runtime
            .as_ref()
            .context("语言服务进程不可用")?
            .server
            .clone();
        Ok(RequestContext {
            server,
            uri: document.uri.clone(),
            snapshot: document.buffer.read(cx).snapshot(),
            buffer_id: document.buffer.entity_id(),
            version: document.version,
            epoch: self.epoch,
            configuration_generation: self.configuration_generation,
            timeout: Duration::from_millis(self.settings.request_timeout_ms),
        })
    }
    pub(super) fn request_is_current(
        &self,
        id: DocumentId,
        request: &RequestContext,
        cx: &App,
    ) -> bool {
        !self.preparing
            && self.configuration_generation == request.configuration_generation
            && self.epoch == request.epoch
            && self.documents.get(&id).is_some_and(|document| {
                document.version == request.version
                    && document.buffer.entity_id() == request.buffer_id
                    && document.buffer.read(cx).text_snapshot().version()
                        == request.snapshot.version()
            })
    }
    pub fn definitions(
        &mut self,
        id: DocumentId,
        position: Anchor,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<DefinitionTarget>>> {
        let request = match self.request_context(id, cx) {
            Ok(request) => request,
            Err(error) => return Task::ready(Err(error)),
        };
        let position = point_to_lsp(position.to_point_utf16(&request.snapshot));
        cx.spawn(async move |this, cx| {
            let response = request
                .server
                .request::<lsp::request::GotoDefinition>(
                    lsp::GotoDefinitionParams {
                        text_document_position_params: lsp::TextDocumentPositionParams {
                            text_document: lsp::TextDocumentIdentifier {
                                uri: request.uri.clone(),
                            },
                            position,
                        },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    },
                    request.timeout,
                )
                .await
                .into_response();
            this.update(cx, |this, cx| {
                if !this.request_is_current(id, &request, cx) {
                    return Ok(Vec::new());
                }
                let response = match response {
                    Ok(response) => response,
                    Err(error) => {
                        this.fail(format!("定义请求失败：{error:#}"), cx);
                        return Err(error);
                    }
                };
                let locations = match response {
                    Some(lsp::GotoDefinitionResponse::Scalar(location)) => {
                        vec![(location.uri, location.range)]
                    }
                    Some(lsp::GotoDefinitionResponse::Array(locations)) => locations
                        .into_iter()
                        .map(|location| (location.uri, location.range))
                        .collect(),
                    Some(lsp::GotoDefinitionResponse::Link(links)) => links
                        .into_iter()
                        .map(|link| (link.target_uri, link.target_selection_range))
                        .collect(),
                    None => Vec::new(),
                };
                locations
                    .into_iter()
                    .map(|(uri, range)| {
                        let document_id = this
                            .documents
                            .iter()
                            .find(|(_, document)| document.uri == uri)
                            .map(|(id, _)| *id);
                        let path = uri
                            .to_file_path()
                            .map_err(|()| anyhow::anyhow!("不支持此定义位置的 URI：{uri}"))?;
                        Ok(DefinitionTarget {
                            document_id,
                            path,
                            range: PointUtf16::new(range.start.line, range.start.character)
                                ..PointUtf16::new(range.end.line, range.end.character),
                        })
                    })
                    .collect()
            })?
        })
    }
}

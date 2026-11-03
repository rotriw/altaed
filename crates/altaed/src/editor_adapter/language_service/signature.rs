//! Protocol conversion stays here; the editor owns cancellation, placement,
//! overload navigation and active-parameter highlighting of its native popover.
use super::*;
use editor::SignatureHelpProvider;
use language::{Anchor, ToPointUtf16, point_to_lsp};

impl SignatureHelpProvider for ClangdProvider {
    fn signature_help(
        &self,
        _: &Entity<Buffer>,
        position: Anchor,
        cx: &mut App,
    ) -> Task<Option<Vec<project::lsp_command::SignatureHelp>>> {
        let id = self.document_id;
        let prepared = self.service.update(cx, |service, cx| {
            if service.preparing || !matches!(service.phase, Phase::Ready) {
                return Ok(None);
            }
            service.request_context(id, cx).map(Some)
        });
        let request = match prepared {
            Ok(Ok(Some(request)))
                if request
                    .server
                    .capabilities()
                    .signature_help_provider
                    .is_some() =>
            {
                request
            }
            Ok(Err(_)) => return Task::ready(None),
            _ => return Task::ready(None),
        };
        let service = self.service.clone();
        let position = point_to_lsp(position.to_point_utf16(&request.snapshot));
        cx.spawn(async move |cx| {
            let response = request
                .server
                .request::<lsp::request::SignatureHelpRequest>(
                    lsp::SignatureHelpParams {
                        text_document_position_params: lsp::TextDocumentPositionParams {
                            text_document: lsp::TextDocumentIdentifier {
                                uri: request.uri.clone(),
                            },
                            position,
                        },
                        context: None,
                        work_done_progress_params: Default::default(),
                    },
                    request.timeout,
                )
                .await
                .into_response();
            service
                .update(cx, |service, cx| {
                    if !service.request_is_current(id, &request, cx) {
                        return None;
                    }
                    let help = match response {
                        Ok(help) => help,
                        Err(error) => {
                            service.fail(format!("clangd 参数提示请求失败：{error:#}"), cx);
                            return None;
                        }
                    };
                    Some(
                        help.and_then(|help| {
                            project::lsp_command::SignatureHelp::new(
                                help,
                                None,
                                Some(request.server.server_id()),
                                cx,
                            )
                        })
                        .into_iter()
                        .collect(),
                    )
                })
                .ok()
                .flatten()
        })
    }
}

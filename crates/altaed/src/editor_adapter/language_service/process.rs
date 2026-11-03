//! Process startup and shutdown are separate from document synchronization.
use super::*;
impl LanguageService {
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        self.stop(cx);
        if !self.settings.enabled {
            self.phase = Phase::Disabled;
            return;
        }
        self.phase = Phase::Starting;
        let epoch = self.epoch;
        let root = self.root.clone();
        let settings = self.settings.clone();
        let drivers = self.drivers.clone();
        let weak = cx.weak_entity();
        self.startup_task = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let server = LanguageServer::new(
                    Arc::new(Mutex::new(None)),
                    LanguageServerId(NEXT_SERVER_ID.fetch_add(1, Ordering::Relaxed)),
                    LanguageServerName("clangd".into()),
                    clangd_binary(&settings, &drivers)?,
                    &root,
                    None,
                    None,
                    cx,
                )?;
                let diagnostics = server
                    .on_notification::<lsp::notification::PublishDiagnostics, _>(
                        move |params, cx| {
                            let _ = weak
                                .update(cx, |this, cx| this.publish_diagnostics(epoch, params, cx));
                        },
                    );
                let initialization = cx.update(|app| {
                    let params = initialize_params(&server, app);
                    server.initialize(
                        params,
                        Arc::new(lsp::DidChangeConfigurationParams {
                            settings: serde_json::json!({}),
                        }),
                        Duration::from_millis(settings.startup_timeout_ms),
                        app,
                    )
                });
                let server = initialization.await?;
                anyhow::ensure!(
                    server
                        .capabilities()
                        .position_encoding
                        .as_ref()
                        .is_none_or(|encoding| *encoding == lsp::PositionEncodingKind::UTF16),
                    "clangd 未使用协商的 UTF-16 坐标编码"
                );
                anyhow::Ok(Runtime {
                    server,
                    _diagnostics: diagnostics,
                })
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                if epoch != this.epoch {
                    return;
                }
                match result {
                    Ok(runtime) => {
                        let server = runtime.server.clone();
                        this.runtime = Some(runtime);
                        let opened = this.send_compilation_commands(&server).and_then(|()| {
                            for id in this.documents.keys().copied().collect::<Vec<_>>() {
                                this.open_document(id, &server, cx)?;
                            }
                            Ok(())
                        });
                        match opened {
                            Ok(()) => {
                                this.phase = Phase::Ready;
                                this.refresh_all_inlay_hints(cx);
                            }
                            Err(error) => this.fail(format!("{error:#}"), cx),
                        }
                    }
                    Err(error) => this.fail(format!("{error:#}"), cx),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn fail(&mut self, error: String, cx: &mut Context<Self>) {
        self.stop(cx);
        self.phase = Phase::Failed(error);
        cx.notify();
    }
    pub(super) fn stop(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        self.startup_task = None;
        for id in self.documents.keys().copied().collect::<Vec<_>>() {
            self.clear_inlay_hints(id, cx);
            self.clear_diagnostics(id, cx);
            if let (Some(runtime), Some(document)) = (&self.runtime, self.documents.get(&id)) {
                document.buffer.update(cx, |buffer, cx| {
                    buffer.set_completion_triggers(runtime.server.server_id(), BTreeSet::new(), cx)
                });
            }
        }
        if let Some(runtime) = self.runtime.take()
            && let Some(shutdown) = runtime.server.shutdown()
        {
            cx.background_spawn(async move {
                shutdown.await;
            })
            .detach();
        }
        for document in self.documents.values_mut() {
            document.sent_version = -1;
            document.pending_change = None;
            document.completion_error = None;
        }
    }
    pub fn shutdown(&mut self, cx: &mut Context<Self>) {
        self.configuration_generation += 1;
        self.preparing_task = None;
        self.preparing = false;
        self.stop(cx);
        self.phase = Phase::Disabled;
    }
}

fn clangd_binary(
    settings: &LanguageServerSettings,
    drivers: &BTreeSet<String>,
) -> Result<LanguageServerBinary> {
    let requested = settings
        .path
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new(crate::language_server::CLANGD_PROGRAM));
    let binary = toolchain::find_executable(requested).with_context(|| {
        format!(
            "找不到 clangd：{}，请在软件设置中指定路径",
            requested.display()
        )
    })?;
    let mut arguments = settings
        .effective_arguments()
        .into_iter()
        .map(Into::into)
        .collect::<Vec<std::ffi::OsString>>();
    if !settings
        .arguments
        .iter()
        .any(|argument| argument.starts_with("--query-driver"))
        && !drivers.is_empty()
    {
        anyhow::ensure!(
            !drivers
                .iter()
                .any(|driver| driver.contains([',', '*', '?', '[', ']'])),
            "编译器路径含有 clangd query-driver 无法安全匹配的字符"
        );
        arguments.push(
            format!(
                "--query-driver={}",
                drivers.iter().cloned().collect::<Vec<_>>().join(",")
            )
            .into(),
        );
    }
    Ok(LanguageServerBinary {
        path: binary,
        arguments,
        env: None,
    })
}

fn initialize_params(server: &LanguageServer, app: &App) -> lsp::InitializeParams {
    let mut params = server.default_initialize_params(false, false, app);
    // Only presentation fields are resolved lazily. Insertion edits must be
    // eager: this adapter does not apply arbitrary workspace changes/commands.
    params.capabilities = lsp::ClientCapabilities {
        general: params.capabilities.general,
        text_document: Some(lsp::TextDocumentClientCapabilities {
            synchronization: Some(lsp::TextDocumentSyncClientCapabilities::default()),
            inlay_hint: Some(lsp::InlayHintClientCapabilities {
                dynamic_registration: Some(false),
                resolve_support: None,
            }),
            completion: Some(lsp::CompletionClientCapabilities {
                context_support: Some(true),
                completion_item: Some(lsp::CompletionItemCapability {
                    snippet_support: Some(true),
                    insert_replace_support: Some(true),
                    label_details_support: Some(true),
                    resolve_support: Some(lsp::CompletionItemCapabilityResolveSupport {
                        properties: vec!["documentation".into(), "detail".into()],
                    }),
                    documentation_format: Some(vec![
                        lsp::MarkupKind::Markdown,
                        lsp::MarkupKind::PlainText,
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            definition: Some(lsp::GotoCapability {
                link_support: Some(true),
                ..Default::default()
            }),
            signature_help: Some(lsp::SignatureHelpClientCapabilities {
                signature_information: Some(lsp::SignatureInformationSettings {
                    documentation_format: Some(vec![
                        lsp::MarkupKind::Markdown,
                        lsp::MarkupKind::PlainText,
                    ]),
                    parameter_information: Some(lsp::ParameterInformationSettings {
                        label_offset_support: Some(true),
                    }),
                    active_parameter_support: Some(true),
                }),
                ..Default::default()
            }),
            publish_diagnostics: Some(lsp::PublishDiagnosticsClientCapabilities {
                version_support: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    params
}

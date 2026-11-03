use super::*;
use language::{PointUtf16, ToOffset};

#[gpui::test]
fn completion_replaces_utf16_range_and_keeps_function_snippets(cx: &mut gpui::TestAppContext) {
    let text = "// 中文🙂\nint main() { /*🙂*/ local_ans; }";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    buffer.read_with(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        let offset = text.find("local_ans").unwrap();
        let start = language::ToPointUtf16::to_point_utf16(&offset, &snapshot);
        let end = PointUtf16::new(start.row, start.column + 9);
        let item = lsp::CompletionItem {
            label: " local_answer()".into(),
            filter_text: Some("local_answer".into()),
            insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
            text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                range: lsp::Range::new(language::point_to_lsp(start), language::point_to_lsp(end)),
                new_text: "local_answer(${1:value})$0".into(),
            })),
            ..Default::default()
        };
        let converted = completion::convert(
            item,
            None,
            LanguageServerId(1),
            &snapshot,
            snapshot.anchor_before(offset + 9),
        )
        .unwrap();
        assert_eq!(converted.replace_range.start.to_offset(&snapshot), offset);
        assert_eq!(converted.replace_range.end.to_offset(&snapshot), offset + 9);
        assert_eq!(converted.new_text, "local_answer(${1:value})$0");
        assert_eq!(converted.label.text, "local_answer()");
        assert_eq!(
            converted
                .source
                .lsp_completion(true)
                .unwrap()
                .insert_text_format,
            Some(lsp::InsertTextFormat::SNIPPET)
        );
        assert!(
            completion::anchors(
                lsp::Range::new(language::point_to_lsp(end), language::point_to_lsp(start)),
                &snapshot
            )
            .is_err()
        );
    });
}

#[gpui::test]
fn completion_types_do_not_pollute_filtering_and_invalid_unicode_edits_are_rejected(
    cx: &mut gpui::TestAppContext,
) {
    let buffer = cx.new(|cx| Buffer::local("/*🙂*/ value", cx));
    buffer.read_with(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        let item = lsp::CompletionItem {
            label: "value".into(),
            detail: Some("int".into()),
            ..Default::default()
        };
        let completion = completion::convert(
            item,
            None,
            LanguageServerId(1),
            &snapshot,
            snapshot.anchor_before(buffer.len()),
        )
        .unwrap();
        assert_eq!(completion.label.text, "value  int");
        assert_eq!(
            &completion.label.text[completion.label.filter_range],
            "value"
        );
        for end in [
            lsp::Position::new(0, 3),
            lsp::Position::new(0, 200),
            lsp::Position::new(1, 0),
        ] {
            assert!(
                completion::anchors(lsp::Range::new(lsp::Position::new(0, 0), end), &snapshot)
                    .is_err()
            );
        }
        let defaults = lsp::CompletionListItemDefaults {
            edit_range: Some(lsp::CompletionListItemDefaultsEditRange::Range(
                lsp::Range::new(lsp::Position::new(0, 7), lsp::Position::new(0, 12)),
            )),
            insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
            ..Default::default()
        };
        let item = lsp::CompletionItem {
            label: "function".into(),
            text_edit_text: Some("function(${1:value})$0".into()),
            ..Default::default()
        };
        let completion = completion::convert(
            item,
            Some(Arc::new(defaults)),
            LanguageServerId(1),
            &snapshot,
            snapshot.anchor_before(buffer.len()),
        )
        .unwrap();
        assert_eq!(completion.new_text, "function(${1:value})$0");
        assert_eq!(completion.replace_range.start.to_offset(&snapshot), 9);
    });
}

// Deterministic transport fixture checks delayed replies independently of the
// separately recorded real-clangd/native checks.
#[gpui::test]
fn diagnostics_and_requests_cannot_cross_versions_or_server_restarts(
    cx: &mut gpui::TestAppContext,
) {
    let (service, buffer, id, uri, _fake) = transport_fixture(cx, "int answer = missing;");
    let diagnostic = |version| lsp::PublishDiagnosticsParams {
        uri: uri.clone(),
        version: Some(version),
        diagnostics: vec![lsp::Diagnostic {
            range: lsp::Range::new(lsp::Position::new(0, 13), lsp::Position::new(0, 20)),
            message: "unknown identifier".into(),
            ..Default::default()
        }],
    };
    service.update(cx, |service, cx| {
        service.publish_diagnostics(0, diagnostic(0), cx)
    });
    let count = |cx: &gpui::TestAppContext| {
        buffer.read_with(cx, |buffer, _| {
            buffer
                .snapshot()
                .diagnostics_in_range::<_, PointUtf16>(0..buffer.len(), false)
                .count()
        })
    };
    assert_eq!(count(cx), 1);
    let request = service.update(cx, |service, cx| service.request_context(id, cx).unwrap());
    buffer.update(cx, |buffer, cx| buffer.edit([(13..20, "42")], None, cx));
    cx.run_until_parked();
    assert_eq!(count(cx), 0);
    service.update(cx, |service, cx| {
        assert!(!service.request_is_current(id, &request, cx));
        service.flush_change(id, cx).unwrap();
        service.publish_diagnostics(0, diagnostic(0), cx);
    });
    assert_eq!(count(cx), 0, "old version must not restore stale errors");
    service.update(cx, |service, cx| {
        service.publish_diagnostics(0, diagnostic(1), cx)
    });
    assert_eq!(count(cx), 1);
    service.update(cx, |service, cx| {
        let current = service.request_context(id, cx).unwrap();
        assert!(service.request_is_current(id, &current, cx));
        service.preparing = true;
        assert!(!service.request_is_current(id, &current, cx));
        service.preparing = false;
        service.shutdown(cx);
        service.publish_diagnostics(0, diagnostic(1), cx);
        assert!(!service.request_is_current(id, &current, cx));
    });
    assert_eq!(
        count(cx),
        0,
        "shutdown must clear diagnostics and reject old server replies"
    );
}

fn transport_fixture(
    cx: &mut gpui::TestAppContext,
    text: &str,
) -> (
    Entity<LanguageService>,
    Entity<Buffer>,
    DocumentId,
    lsp::Uri,
    lsp::FakeLanguageServer,
) {
    let root = std::env::temp_dir().join(format!("altaed-lsp-versions-{}", uuid::Uuid::new_v4()));
    let document = CodeDocument::untitled();
    let id = document.id;
    let plan = DocumentPlan::build(&document, &root, None).unwrap();
    let uri = plan.uri().unwrap();
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let service = cx.update(|cx| LanguageService::new(root, cx));
    let (server, fake) = lsp::FakeLanguageServer::new(
        LanguageServerId(3),
        LanguageServerBinary {
            path: "deterministic-transport-fixture".into(),
            arguments: Vec::new(),
            env: None,
        },
        "fixture".into(),
        Default::default(),
        &mut cx.to_async(),
    );
    let server = Arc::new(server);
    let diagnostics = server.on_notification::<lsp::notification::PublishDiagnostics, _>(|_, _| {});
    service.update(cx, |service, cx| {
        let edits = cx.subscribe(&buffer, move |service, _, event, cx| {
            if matches!(event, BufferEvent::Edited) {
                service.buffer_edited(id, cx);
            }
        });
        service.documents.insert(
            id,
            DocumentState {
                plan,
                uri: uri.clone(),
                buffer: buffer.clone(),
                version: 0,
                sent_version: 0,
                sent_snapshot: buffer.read(cx).text_snapshot(),
                observed_snapshot: buffer.read(cx).text_snapshot(),
                _edits: edits,
                pending_change: None,
                completion_error: None,
                hints: Default::default(),
            },
        );
        service.runtime = Some(Runtime {
            server,
            _diagnostics: diagnostics,
        });
        service.phase = Phase::Ready;
    });
    (service, buffer, id, uri, fake)
}

#[gpui::test]
fn inlay_hints_are_display_only_filtered_and_cancelled_after_edits(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let store = settings::SettingsStore::test(cx);
        cx.set_global(store);
        assets::Assets.load_test_fonts(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
    });
    let text = "/*🙂*/ auto answer = combine(4);";
    let (service, buffer, id, uri, fake) = transport_fixture(cx, text);
    let (editor, cx) = cx
        .add_window_view(|window, cx| editor::Editor::for_buffer(buffer.clone(), None, window, cx));
    service.update(cx, |service, cx| {
        service.hint_settings.enabled = true;
        service.documents.get_mut(&id).unwrap().hints.editor = Some(editor.downgrade());
        service
            .runtime
            .as_ref()
            .unwrap()
            .server
            .update_capabilities(|caps| {
                caps.inlay_hint_provider = Some(lsp::OneOf::Left(true));
            });
        service.refresh_all_inlay_hints(cx);
    });
    let hints: Vec<lsp::InlayHint> = serde_json::from_value(serde_json::json!([
        {"position":{"line":0,"character":18},"label":": int","kind":1},
        {"position":{"line":0,"character":29},"label":[{"value":"count:","command":{"title":"ignored","command":"never.execute"}}],"kind":2,"paddingRight":true},
        {"position":{"line":0,"character":3},"label":"invalid UTF-16","kind":2},
        {"position":{"line":0,"character":29},"label":"invalid\nmultiline","kind":2}
    ])).unwrap();
    let expected_uri = uri.clone();
    fake.set_request_handler::<lsp::request::InlayHintRequest, _, _>(move |params, _| {
        assert_eq!(params.text_document.uri, expected_uri);
        assert_eq!(params.range.end, lsp::Position::new(0, 32));
        let hints = hints.clone();
        async move { Ok(Some(hints)) }
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    let visible = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.snapshot(window, cx).display_snapshot.text()
            })
        })
    };
    let rendered = visible(cx);
    assert!(rendered.contains(": int"), "{rendered}");
    assert!(rendered.contains("count: "), "{rendered}");
    assert!(!rendered.contains("invalid"), "{rendered}");
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        text,
        "hints must not enter the source"
    );

    service.update(cx, |service, cx| {
        service.hint_settings.types = false;
        service.refresh_all_inlay_hints(cx);
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    assert!(!visible(cx).contains(": int"));
    assert!(visible(cx).contains("count: "));

    let (release, wait) = futures::channel::oneshot::channel();
    let mut wait = Some(wait);
    fake.set_request_handler::<lsp::request::InlayHintRequest, _, _>(move |_, _| {
        let wait = wait.take().unwrap();
        async move {
            wait.await?;
            Ok(Some(serde_json::from_value(serde_json::json!([{"position":{"line":0,"character":29},"label":"stale:","kind":2}]))?))
        }
    });
    service.update(cx, |service, cx| service.refresh_all_inlay_hints(cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "// changed\n")], None, cx)
    });
    let _ = release.send(());
    cx.run_until_parked();
    assert!(!visible(cx).contains("stale:"));
    service.update(cx, |service, cx| {
        service.hint_settings.enabled = false;
        service.refresh_all_inlay_hints(cx);
    });
    assert_eq!(visible(cx), buffer.read_with(cx, |buffer, _| buffer.text()));
    service.update(cx, |service, cx| service.shutdown(cx));
}

#[gpui::test]
async fn signature_provider_uses_utf16_current_parameter_and_discards_late_replies(
    cx: &mut gpui::TestAppContext,
) {
    use editor::SignatureHelpProvider;
    let (service, buffer, id, uri, fake) = transport_fixture(cx, "/*🙂*/ combine(1, )");
    service.update(cx, |service, _| {
        service
            .runtime
            .as_ref()
            .unwrap()
            .server
            .update_capabilities(|caps| {
                caps.signature_help_provider = Some(Default::default());
            })
    });
    let provider = cx.update(|_| ClangdProvider {
        service: service.downgrade(),
        document_id: id,
    });
    let position = buffer.read_with(cx, |buffer, _| {
        buffer.snapshot().anchor_before(buffer.len() - 1)
    });
    let expected_uri = uri.clone();
    fake.set_request_handler::<lsp::request::SignatureHelpRequest, _, _>(move |params, _| {
        assert_eq!(
            params.text_document_position_params.text_document.uri,
            expected_uri
        );
        assert_eq!(
            params.text_document_position_params.position,
            lsp::Position::new(0, 18)
        );
        async move {
            Ok(Some(lsp::SignatureHelp {
                signatures: vec![lsp::SignatureInformation {
                    label: "combine(int count, double weight)".into(),
                    parameters: Some(vec![
                        lsp::ParameterInformation {
                            label: lsp::ParameterLabel::LabelOffsets([8, 17]),
                            documentation: None,
                        },
                        lsp::ParameterInformation {
                            label: lsp::ParameterLabel::LabelOffsets([19, 32]),
                            documentation: None,
                        },
                    ]),
                    documentation: None,
                    active_parameter: None,
                }],
                active_signature: Some(0),
                active_parameter: Some(1),
            }))
        }
    });
    let task = cx.update(|cx| provider.signature_help(&buffer, position, cx));
    let help = task.await.unwrap();
    assert_eq!(help[0].signatures[0].active_parameter, Some(1));
    assert_eq!(help[0].signatures[0].highlights[0].0, 19..32);

    let (release, wait) = futures::channel::oneshot::channel();
    let mut wait = Some(wait);
    fake.set_request_handler::<lsp::request::SignatureHelpRequest, _, _>(move |_, _| {
        let wait = wait.take().unwrap();
        async move {
            wait.await?;
            Ok(Some(lsp::SignatureHelp {
                signatures: Vec::new(),
                active_signature: None,
                active_parameter: None,
            }))
        }
    });
    let late = cx.update(|cx| provider.signature_help(&buffer, position, cx));
    cx.run_until_parked();
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "// changed\n")], None, cx)
    });
    release.send(()).unwrap();
    assert!(
        late.await.is_none(),
        "old content must not reopen a parameter popover"
    );
    service.update(cx, |service, cx| service.shutdown(cx));
}

#[gpui::test]
fn typing_triggers_semantic_completion_and_a_failed_query_does_not_stop_clangd(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let store = settings::SettingsStore::test(cx);
        cx.set_global(store);
        assets::Assets.load_test_fonts(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        crate::keymap::init(&crate::configuration::AppPreferences::default(), cx).unwrap();
    });
    let prefix = "int main() { ans";
    let (service, buffer, id, uri, fake) = transport_fixture(cx, prefix);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = editor::Editor::for_buffer(buffer.clone(), None, window, cx);
        editor.set_completion_provider(Some(Rc::new(ClangdProvider {
            service: service.downgrade(),
            document_id: id,
        })));
        editor.set_show_completions_on_input(Some(true));
        editor.change_selections(
            editor::SelectionEffects::no_scroll().completions(false),
            window,
            cx,
            |selections| {
                let end = editor::MultiBufferOffset(prefix.len());
                selections.select_ranges([end..end]);
            },
        );
        editor
    });
    cx.update(|window, cx| {
        use gpui::Focusable;
        editor.focus_handle(cx).focus(window, cx);
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let received = calls.clone();
    fake.set_request_handler::<lsp::request::Completion, _, _>(move |params, _| {
        assert_eq!(params.text_document_position.text_document.uri, uri);
        let call = received.fetch_add(1, Ordering::Relaxed);
        async move {
            if call == 0 {
                anyhow::bail!("transient completion error fixture");
            }
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "answer(int limit)".into(),
                    filter_text: Some("answer".into()),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::new(
                            lsp::Position::new(0, 13),
                            params.text_document_position.position,
                        ),
                        new_text: "answer(${1:limit})$0".into(),
                    })),
                    ..Default::default()
                },
            ])))
        }
    });
    cx.simulate_keystrokes("w");
    cx.run_until_parked();
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "typing must request completion without a completion shortcut"
    );
    service.read_with(cx, |service, _| {
        assert!(matches!(service.phase, Phase::Ready));
        assert!(service.documents[&id].completion_error.is_some());
    });
    cx.simulate_keystrokes("e");
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "int main() { answer(limit)"
    );
    service.read_with(cx, |service, _| {
        assert!(service.documents[&id].completion_error.is_none())
    });

    editor.update(cx, |editor, _| {
        editor.set_show_completions_on_input(Some(false))
    });
    cx.simulate_keystrokes("z");
    cx.run_until_parked();
    assert_eq!(
        calls.load(Ordering::Relaxed),
        2,
        "automatic completion must respect the software preference"
    );
    service.update(cx, |service, cx| service.shutdown(cx));
}

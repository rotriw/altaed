use super::*;
use gpui::{Modifiers, TestAppContext, VisualTestContext, point, size};

fn init_test_editor(cx: &mut TestAppContext) -> Arc<Language> {
    cx.update(|cx| {
        let store = settings::SettingsStore::test(cx);
        cx.set_global(store);
        assets::Assets.load_test_fonts(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
    });
    Arc::new(Language::new(
        toml::from_str(include_str!("../../../../languages/cpp/config.toml")).unwrap(),
        None,
    ))
}

fn drag(cx: &mut VisualTestContext, from: &str, to: &str) {
    let bounds = |cx: &mut VisualTestContext, name: &str| match name {
        "first" => cx.debug_bounds("TAB-document-tab-0").unwrap(),
        "last" => cx.debug_bounds("TAB-document-tab-2").unwrap(),
        "end" => cx.debug_bounds("tab-drop-end").unwrap(),
        _ => panic!("unknown tab fixture"),
    };
    let start = bounds(cx, from).center();
    let end = bounds(cx, to).center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        start + point(px(8.), px(0.)),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

#[gpui::test]
fn native_drag_moves_views_with_documents_preserves_identity_and_restores_order(
    cx: &mut TestAppContext,
) {
    let language = init_test_editor(cx);
    let directory = std::env::temp_dir().join(format!("altaed-drag-{}", uuid::Uuid::new_v4()));

    let mut documents = Vec::new();
    for name in ["A", "B", "C"] {
        let mut document = CodeDocument::untitled();
        document.text = format!("// {name}\n");
        document.metadata.tests.push(crate::document::TestCase {
            input: name.into(),
            expected_output: None,
        });
        documents.push(document);
    }
    let ids: Vec<_> = documents.iter().map(|document| document.id).collect();
    let (ide, cx) = cx.add_window_view(|window, cx| {
        Ide::new(
            Session {
                documents,
                active: 1,
            },
            directory.clone(),
            language,
            window,
            cx,
        )
    });
    cx.simulate_resize(size(px(1250.), px(800.)));
    cx.run_until_parked();
    let buffers = ide.read_with(cx, |ide, _| {
        ide.views
            .iter()
            .map(|view| view.buffer.clone())
            .collect::<Vec<_>>()
    });
    let names = ide.read_with(cx, |ide, _| {
        ide.session
            .documents
            .iter()
            .map(CodeDocument::display_name)
            .collect::<Vec<_>>()
    });
    drag(cx, "first", "last");
    ide.read_with(cx, |ide, _| {
        assert_eq!(
            ide.session
                .documents
                .iter()
                .map(|document| document.id)
                .collect::<Vec<_>>(),
            vec![ids[1], ids[2], ids[0]]
        );
        assert_eq!(ide.session.documents[ide.session.active].id, ids[1]);
        assert_eq!(ide.views[0].buffer, buffers[1]);
        assert_eq!(ide.views[1].buffer, buffers[2]);
        assert_eq!(ide.views[2].buffer, buffers[0]);
        assert_eq!(ide.session.documents[2].display_name(), names[0]);
        assert_eq!(ide.session.documents[2].metadata.tests[0].input, "A");
    });
    buffers[2].update(cx, |buffer, cx| {
        buffer.edit([(0..buffer.len(), "// edited C\n")], None, cx)
    });
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.session.documents[1].text, "// edited C\n");
        assert_eq!(ide.session.documents[2].text, "// A\n");
        crate::storage::save_session(&directory, &ide.session).unwrap();
    });
    let mut restored = crate::storage::load_session(&directory).unwrap();
    restored.ensure_document();
    assert_eq!(
        restored
            .documents
            .iter()
            .map(|document| document.id)
            .collect::<Vec<_>>(),
        vec![ids[1], ids[2], ids[0]]
    );
    assert_eq!(restored.documents[2].display_name(), names[0]);
    drag(cx, "last", "first");
    ide.read_with(cx, |ide, _| {
        assert_eq!(
            ide.session
                .documents
                .iter()
                .map(|document| document.id)
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(ide.session.documents[ide.session.active].id, ids[1]);
    });
    // Escape must cancel before mouse release; no reordering should commit.
    let first = cx.debug_bounds("TAB-document-tab-0").unwrap().center();
    let last = cx.debug_bounds("TAB-document-tab-2").unwrap().center();
    cx.simulate_mouse_down(first, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(last, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(last, gpui::MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(
            ide.session
                .documents
                .iter()
                .map(|document| document.id)
                .collect::<Vec<_>>(),
            ids
        )
    });
    drag(cx, "first", "end");
    ide.read_with(cx, |ide, _| {
        assert_eq!(
            ide.session
                .documents
                .iter()
                .map(|document| document.id)
                .collect::<Vec<_>>(),
            vec![ids[1], ids[2], ids[0]]
        );
        assert_eq!(ide.session.documents[ide.session.active].id, ids[1]);
    });
    std::fs::remove_dir_all(directory).unwrap();
}

#[gpui::test]
fn inactive_close_preserves_current_document_and_editor_focus(cx: &mut TestAppContext) {
    let language = init_test_editor(cx);

    let documents = ["first.cpp", "current.cpp", "last.cpp"]
        .into_iter()
        .map(|name| {
            let mut document = CodeDocument::untitled();
            document.path = Some(std::env::temp_dir().join(name));
            document
        })
        .collect::<Vec<_>>();
    let current = documents[1].id;
    let directory =
        std::env::temp_dir().join(format!("altaed-inactive-close-{}", uuid::Uuid::new_v4()));
    let (ide, cx) = cx.add_window_view(|window, cx| {
        Ide::new(
            Session {
                documents,
                active: 1,
            },
            directory,
            language,
            window,
            cx,
        )
    });
    let editor = ide.read_with(cx, |ide, _| ide.views[1].editor.clone());
    let first = cx.debug_bounds("TAB-document-tab-0").unwrap();
    cx.simulate_click(
        point(first.right() - px(12.), first.center().y),
        Modifiers::default(),
    );
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.session.documents.len(), 2);
        assert_eq!(ide.session.documents[ide.session.active].id, current);
        assert_eq!(ide.views[ide.session.active].editor, editor);
    });
    let last = cx.debug_bounds("TAB-document-tab-1").unwrap().center();
    cx.simulate_mouse_down(last, gpui::MouseButton::Middle, Modifiers::default());
    cx.simulate_mouse_up(last, gpui::MouseButton::Middle, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        ide.read_with(cx, |ide, cx| {
            assert_eq!(ide.session.documents.len(), 1);
            assert_eq!(ide.session.documents[ide.session.active].id, current);
            assert_eq!(window.focused(cx), Some(editor.focus_handle(cx)));
        });
    });
}

#[gpui::test]
fn held_drag_scrolls_at_edge_and_stops_after_leaving_cancelling_and_releasing(
    cx: &mut TestAppContext,
) {
    let language = init_test_editor(cx);

    let documents = (0..20)
        .map(|index| {
            let mut document = CodeDocument::untitled();
            document.path =
                Some(std::env::temp_dir().join(format!("solution_{index}_with_long_name.cpp")));
            document
        })
        .collect();
    let directory =
        std::env::temp_dir().join(format!("altaed-drag-scroll-{}", uuid::Uuid::new_v4()));
    let (ide, cx) = cx.add_window_view(|window, cx| {
        Ide::new(
            Session {
                documents,
                active: 0,
            },
            directory,
            language,
            window,
            cx,
        )
    });
    cx.simulate_resize(size(px(900.), px(500.)));
    cx.run_until_parked();
    let ids = ide.read_with(cx, |ide, _| {
        ide.session
            .documents
            .iter()
            .map(|document| document.id)
            .collect::<Vec<_>>()
    });
    let start = cx.debug_bounds("TAB-document-tab-0").unwrap().center();
    let viewport = ide.read_with(cx, |ide, _| ide.tabs_scroll.bounds());
    let edge = point(viewport.right() - px(2.), viewport.center().y);
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        start + point(px(8.), px(0.)),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(edge, gpui::MouseButton::Left, Modifiers::default());
    for _ in 0..12 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.run_until_parked();
    }
    let scrolled = ide.read_with(cx, |ide, _| ide.tabs_scroll.offset().x);
    assert!(
        scrolled < px(-80.),
        "holding the pointer must scroll without more mouse events: {scrolled:?}"
    );
    cx.simulate_mouse_move(
        viewport.center(),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.run_until_parked();
    }
    assert_eq!(
        ide.read_with(cx, |ide, _| ide.tabs_scroll.offset().x),
        scrolled
    );
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(
        viewport.center(),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    let cancelled = ide.read_with(cx, |ide, _| ide.tabs_scroll.offset().x);
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.run_until_parked();
    }
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.tabs_scroll.offset().x, cancelled);
        assert_eq!(
            ide.session
                .documents
                .iter()
                .map(|document| document.id)
                .collect::<Vec<_>>(),
            ids
        );
    });
    // A normal release ends the native drag lifetime too, including a drop
    // outside the tab bar where no reorder handler runs.
    let start = cx.debug_bounds("TAB-document-tab-0").unwrap().center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(edge, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(
        point(edge.x, edge.y + px(100.)),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    let released = ide.read_with(cx, |ide, _| ide.tabs_scroll.offset().x);
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.run_until_parked();
    }
    assert_eq!(
        ide.read_with(cx, |ide, _| ide.tabs_scroll.offset().x),
        released
    );
}

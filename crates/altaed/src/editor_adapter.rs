pub(crate) mod language_service;
use editor::Editor;
use gpui::{App, AppContext, Entity, Window};
use language::{Buffer, Language};
use std::sync::Arc;

pub struct EditorView {
    pub buffer: Entity<Buffer>,
    pub editor: Entity<Editor>,
}
impl EditorView {
    pub fn apply_preferences(
        &self,
        preferences: &crate::configuration::EditorPreferences,
        cx: &mut App,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.set_text_style_refinement(gpui::TextStyleRefinement {
                font_family: Some(preferences.font_family.clone().into()),
                font_size: Some(gpui::px(preferences.font_size).into()),
                ..Default::default()
            });
            editor.set_show_line_numbers(preferences.line_numbers, cx);
            editor.set_auto_signature_help(preferences.auto_signature_help, cx);
            editor.set_soft_wrap_mode(
                if preferences.soft_wrap {
                    language::language_settings::SoftWrap::EditorWidth
                } else {
                    language::language_settings::SoftWrap::None
                },
                cx,
            );
            cx.notify();
        });
    }
    pub fn new(text: String, language: Arc<Language>, window: &mut Window, cx: &mut App) -> Self {
        let buffer = cx.new(|cx| {
            let mut buffer = Buffer::local(text, cx);
            buffer.set_language(Some(language), cx);
            buffer
        });
        let editor = cx.new(|cx| Editor::for_buffer(buffer.clone(), None, window, cx));
        Self { buffer, editor }
    }
    pub fn text(&self, cx: &App) -> String {
        self.buffer.read(cx).text()
    }
}

impl EditorView {
    pub fn cursor_offset(&self, cx: &App) -> usize {
        use editor::ToOffset;
        let editor = self.editor.read(cx);
        editor
            .selections
            .newest_anchor()
            .head()
            .to_offset(&editor.buffer().read(cx).snapshot(cx))
            .0
    }
    pub fn select_range(
        &self,
        range: std::ops::Range<language::PointUtf16>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let snapshot = self.buffer.read(cx).text_snapshot();
        let start =
            snapshot.clip_point_utf16(language::Unclipped(range.start), language::Bias::Left);
        let end = snapshot.clip_point_utf16(language::Unclipped(range.end), language::Bias::Right);
        self.editor.update(cx, |editor, cx| {
            editor.change_selections(
                editor::SelectionEffects::scroll(editor::scroll::Autoscroll::newest())
                    .completions(false),
                window,
                cx,
                |selections| selections.select_ranges([start..end]),
            );
        });
    }
}

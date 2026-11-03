//! Plain form input, isolated from the IDE's code editor adapter.
//! Reuses the existing input engine for IME, selection, clipboard and undo.
use editor::{Editor, EditorEvent};
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Render,
    Subscription, Window, div, prelude::*,
};

#[cfg(test)]
pub fn init(cx: &mut App) {
    crate::keymap::bind_test_defaults(cx).unwrap();
}
pub struct Changed;
pub struct TextField {
    input: Entity<Editor>,
    multiline: bool,
    read_only: bool,
    padding: f32,
    horizontal_padding: f32,
    _changes: Subscription,
}
impl EventEmitter<Changed> for TextField {}
impl TextField {
    pub fn multiline(
        text: String,
        read_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::create(text, true, read_only, window, cx)
    }
    pub fn single_line(text: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::create(text, false, false, window, cx)
    }
    fn create(
        text: String,
        multiline: bool,
        read_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut input = if multiline {
                Editor::auto_height(3, 4, window, cx)
            } else {
                Editor::single_line(window, cx)
            };
            input.set_text_style_refinement(gpui::TextStyleRefinement {
                font_family: Some(crate::platform::code_font().into()),
                font_size: Some(gpui::px(12.).into()),
                line_height: Some(gpui::px(20.).into()),
                color: Some(crate::panel_style::palette(cx).text),
                ..Default::default()
            });
            input.set_autoindent(false);
            input.set_use_autoclose(false);
            input.set_use_auto_surround(false);
            input.set_show_completions_on_input(Some(false));
            input.set_text(text, window, cx);
            input.set_read_only(read_only);
            input
        });
        let changes = cx.subscribe(&input, |_, _, event, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.emit(Changed);
            }
        });
        Self {
            input,
            multiline,
            read_only,
            padding: 7.,
            horizontal_padding: 7.,
            _changes: changes,
        }
    }
    pub fn text(&self, cx: &App) -> String {
        self.input.read(cx).text(cx)
    }
    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.read_only = read_only;
        self.input
            .update(cx, |input, _| input.set_read_only(read_only));
        cx.notify();
    }
    pub fn set_placeholder(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_placeholder_text(text, window, cx));
    }
    pub fn set_padding_xy(&mut self, horizontal: f32, vertical: f32) {
        self.padding = vertical;
        self.horizontal_padding = horizontal;
    }
    pub fn use_ui_font(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use settings::Settings;
        let family = theme::ThemeSettings::get_global(cx).ui_font.family.clone();
        self.input.update(cx, |input, _| {
            input.set_text_style_refinement(gpui::TextStyleRefinement {
                font_family: Some(family),
                font_size: Some(gpui::px(11.).into()),
                line_height: Some(gpui::px(20.).into()),
                ..Default::default()
            })
        });
        let _ = window;
    }
    pub fn set_text(
        &mut self,
        text: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input
            .update(cx, |input, cx| input.set_text(text.into(), window, cx));
    }
}
impl Focusable for TextField {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}
impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = crate::panel_style::palette(cx);
        div()
            .bg(if self.read_only { c.readonly } else { c.paper })
            .when(self.multiline, |element| {
                element.key_context("AltaedMultiline")
            })
            .size_full()
            .py(gpui::px(self.padding))
            .px(gpui::px(self.horizontal_padding))
            .overflow_hidden()
            .child(self.input.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn enter_in_multiline_inserts_a_newline(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let settings = settings::SettingsStore::test(cx);
            cx.set_global(settings);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            init(cx);
        });
        let (field, cx) =
            cx.add_window_view(|window, cx| TextField::multiline("abc".into(), false, window, cx));
        cx.update(|window, cx| window.focus(&field.focus_handle(cx), cx));
        cx.simulate_keystrokes("enter");
        assert!(
            field
                .read_with(cx, |field, cx| field.text(cx))
                .contains('\n')
        );
    }
    #[gpui::test]
    fn configured_newline_shadows_inherited_editor_enter(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let store = settings::SettingsStore::test(cx);
            cx.set_global(store);
            assets::Assets.load_test_fonts(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            let mut preferences = crate::configuration::AppPreferences::default();
            preferences.keybindings.insert(
                crate::configuration::commands::CommandId("input.newline".into()),
                "ctrl-enter".into(),
            );
            preferences.keybindings.insert(
                crate::configuration::commands::CommandId("input.newline_alternate".into()),
                String::new(),
            );
            crate::keymap::init(&preferences, cx).unwrap();
        });
        let (field, cx) =
            cx.add_window_view(|window, cx| TextField::multiline("abc".into(), false, window, cx));
        cx.update(|window, cx| window.focus(&field.focus_handle(cx), cx));
        cx.simulate_keystrokes("enter");
        assert_eq!(field.read_with(cx, |field, cx| field.text(cx)), "abc");
        cx.simulate_keystrokes("ctrl-enter");
        assert!(
            field
                .read_with(cx, |field, cx| field.text(cx))
                .contains('\n')
        );
    }
}

//! Values translated from the approved prototypes/test-panel/style.css.
use gpui::{App, Div, Hsla, Stateful, div, prelude::*, px, rems, rgb};
use settings::Settings;
use theme::{ActiveTheme, ThemeSettings};
use ui::{Button, ButtonCommon, ButtonSize, ButtonStyle, Color, LabelSize};
use ui::{Clickable, Tooltip};

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub paper: Hsla,
    pub success: Hsla,
    pub subtle: Hsla,
    pub readonly: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub line: Hsla,
    pub accent: Hsla,
    pub selection: Hsla,
    pub error: Hsla,
}
pub(crate) fn palette(cx: &App) -> Palette {
    let c = cx.theme().colors();
    if cx.theme().appearance().is_light() {
        Palette {
            paper: rgb(0xffffff).into(),
            success: rgb(0x247347).into(),
            subtle: rgb(0xf6f7f9).into(),
            readonly: rgb(0xf9fafb).into(),
            text: rgb(0x27313b).into(),
            muted: rgb(0x5b6672).into(),
            line: rgb(0xdfe3e8).into(),
            accent: rgb(0x315e91).into(),
            selection: rgb(0xe7eef7).into(),
            error: rgb(0xa83232).into(),
        }
    } else {
        Palette {
            paper: c.editor_background,
            success: cx.theme().status().success,
            subtle: c.surface_background,
            readonly: c.panel_background,
            text: c.text,
            muted: c.text_muted,
            line: c.border,
            accent: c.text_accent,
            selection: c.element_selected,
            error: cx.theme().status().error,
        }
    }
}
pub(crate) fn button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    cx: &App,
) -> Button {
    let font_size = ThemeSettings::get_global(cx).ui_font_size(cx);
    Button::new(id, label)
        .style(ButtonStyle::Transparent)
        .size(ButtonSize::Compact)
        .label_size(LabelSize::Custom(rems(px(12.) / font_size)))
        .color(Color::Custom(palette(cx).muted))
        .tab_index(0_isize)
}
/// Commands use the same dispatch path from buttons, menus and user shortcuts.
/// Tooltip key labels come from the live keymap instead of copied shortcut text.
pub(crate) fn command_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    description: &'static str,
    action: impl gpui::Action,
    cx: &App,
) -> Button {
    let tooltip_action = action.boxed_clone();
    button(id, label, cx)
        .tooltip(move |_, cx| Tooltip::for_action(description, tooltip_action.as_ref(), cx))
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

// One restrained control vocabulary, with keyboard activation supplied by GPUI's
// focusable click target. Hover changes foreground only, never the surface.
fn form_button_base(
    id: impl Into<gpui::ElementId>,
    label: &str,
    height: f32,
    palette: Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .focusable()
        .tab_index(0_isize)
        .h(px(height))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .px(px(8.))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_size(px(12.))
        .line_height(px(20.))
        .text_color(palette.muted)
        .cursor_pointer()
        .focus_visible(|style| style.border_color(palette.accent))
        .when(!label.is_empty(), |button| button.child(label.to_owned()))
}

/// Install interaction styles exactly once: GPUI rejects a second `hover`
/// refinement instead of overriding the first one.
pub(crate) fn form_button(
    id: impl Into<gpui::ElementId>,
    label: &str,
    height: f32,
    palette: Palette,
) -> Stateful<Div> {
    form_button_base(id, label, height, palette).hover(|style| style.text_color(palette.accent))
}
pub(crate) fn primary_form_button(
    id: impl Into<gpui::ElementId>,
    label: &str,
    height: f32,
    palette: Palette,
) -> Stateful<Div> {
    form_button_base(id, label, height, palette)
        .border_color(palette.accent)
        .bg(palette.accent)
        .text_color(gpui::white())
        .hover(|style| style.text_color(gpui::white()))
}

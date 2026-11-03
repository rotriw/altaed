//! Shared native window chrome for the IDE and settings. State lives in this
//! view so window movement never becomes document or settings-form state.
use gpui::{
    AnyElement, App, Context, IntoElement, MouseButton, ParentElement, Render, Styled, Window,
    WindowControlArea, div, prelude::*, px,
};
use theme::ActiveTheme;

pub(super) fn options() -> gpui::TitlebarOptions {
    gpui::TitlebarOptions {
        title: None,
        appears_transparent: cfg!(any(target_os = "macos", target_os = "windows")),
        traffic_light_position: Some(gpui::point(px(9.), px(9.))),
    }
}
pub(super) fn height(window: &Window) -> gpui::Pixels {
    ui::utils::platform_title_bar_height(window)
}

pub(super) struct WindowChrome {
    drag_armed: bool,
    left: Option<AnyElement>,
    right: Option<AnyElement>,
}
impl WindowChrome {
    pub(super) fn new() -> Self {
        Self {
            drag_armed: false,
            left: None,
            right: None,
        }
    }
    pub(super) fn content(&mut self, left: AnyElement, right: AnyElement) {
        self.left = Some(left);
        self.right = Some(right);
    }
}
impl Render for WindowChrome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let background = if window.is_window_active() {
            colors.title_bar_background
        } else {
            colors.title_bar_inactive_background
        };
        let drag_area = div()
            .id("title-bar-drag")
            .debug_selector(|| "title-bar-drag".into())
            .flex_1()
            .h_full()
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag_armed = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag_armed = false),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, _| this.drag_armed = false))
            .on_mouse_move(cx.listener(|this, _, window, _| {
                if this.drag_armed {
                    this.drag_armed = false;
                    window.start_window_move();
                }
            }))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            });
        div()
            .id("window-title-bar")
            .debug_selector(|| "window-title-bar".into())
            .w_full()
            .h(height(window))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(background)
            .border_b_1()
            .border_color(colors.border)
            .pl(px(
                if cfg!(target_os = "macos") && !window.is_fullscreen() {
                    ui::utils::TRAFFIC_LIGHT_PADDING
                } else {
                    8.
                },
            ))
            .pr(px(if cfg!(target_os = "windows") { 0. } else { 8. }))
            .gap(px(8.))
            .children(self.left.take())
            .child(drag_area)
            .children(self.right.take())
            .children(windows_controls(window, cx))
    }
}

// GPUI routes these native hit-test areas to Windows caption actions (including
// Snap Layouts). macOS keeps the operating system's traffic lights, not replicas.
#[cfg(target_os = "windows")]
fn windows_controls(window: &Window, cx: &App) -> Option<AnyElement> {
    if window.is_fullscreen() {
        return None;
    }
    let colors = cx.theme().colors();
    let mut controls = div()
        .flex()
        .h_full()
        .font_family("Segoe MDL2 Assets")
        .text_size(px(10.));
    for (id, glyph, area) in [
        ("window-minimize", "\u{e921}", WindowControlArea::Min),
        (
            "window-maximize",
            if window.is_maximized() {
                "\u{e923}"
            } else {
                "\u{e922}"
            },
            WindowControlArea::Max,
        ),
        ("window-close", "\u{e8bb}", WindowControlArea::Close),
    ] {
        let close_button = id == "window-close";
        controls = controls.child(
            div()
                .id(id)
                .w(px(36.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .window_control_area(area)
                .text_color(colors.text)
                .hover(move |style| {
                    if close_button {
                        style.bg(gpui::rgb(0xe81123)).text_color(gpui::white())
                    } else {
                        style.text_color(colors.text_accent)
                    }
                })
                .child(glyph),
        );
    }
    Some(controls.into_any_element())
}
#[cfg(not(target_os = "windows"))]
fn windows_controls(_: &Window, _: &App) -> Option<AnyElement> {
    None
}

//! Recoverable startup failures are shown in the app without modifying the
//! user's configuration. File syntax errors must not turn into a silent exit.
use gpui::{
    App, AppContext, Bounds, Context, IntoElement, Render, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, size,
};
use ui::Clickable;

pub fn show(message: String, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(640.), px(320.)), cx);
    if let Err(error) = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("启动失败 — altaed");
            window.on_window_should_close(cx, |_, cx| {
                cx.quit();
                true
            });
            theme::setup_ui_font(window, cx);
            cx.new(|_| StartupError { message })
        },
    ) {
        eprintln!("无法显示启动错误：{error:#}");
        cx.quit();
    }
    cx.activate(true);
}
struct StartupError {
    message: String,
}
impl Render for StartupError {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::panel_style::palette(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .p(px(24.))
            .gap(px(16.))
            .bg(colors.paper)
            .text_color(colors.text)
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("无法启动 altaed"),
            )
            .child(
                div()
                    .id("startup-error")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .text_size(px(12.))
                    .line_height(px(20.))
                    .child(self.message.clone()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(colors.muted)
                    .child("请修正提示的配置后重新启动。现有配置和代码文件均已保留。"),
            )
            .child(
                crate::panel_style::button("quit-after-startup-error", "退出", cx)
                    .on_click(|_, _, cx| cx.quit()),
            )
    }
}

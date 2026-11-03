//! A compact status surface for the active document. Progress belongs to the
//! execution request, while colors belong to application preferences.
use super::*;
use crate::runner::ExecutionPhase;
use gpui::{AnyElement, div, px, rgb};
use settings::Settings;
use ui::prelude::*;

impl Ide {
    pub(super) fn render_status_bar(
        &self,
        document: &CodeDocument,
        cx: &Context<Self>,
    ) -> AnyElement {
        let palette = crate::panel_style::palette(cx);
        let phase = self
            .runs
            .get(&document.id)
            .filter(|run| run.running)
            .map(|run| run.phase);
        let configured = phase.and_then(|phase| {
            let custom = match phase {
                ExecutionPhase::Compiling => self.preferences.workbench.compiling_color,
                ExecutionPhase::Running => self.preferences.workbench.running_color,
            };
            custom.or_else(|| {
                crate::color_theme::execution_color(&self.preferences.editor.theme, phase).ok()
            })
        });
        let background = configured
            .map(|color| rgb(color.rgb()).into())
            .unwrap_or(palette.paper);
        let foreground = configured
            .map(|color| rgb(color.readable_foreground().rgb()).into())
            .unwrap_or(palette.muted);
        let message = match phase {
            Some(ExecutionPhase::Compiling) => "正在编译…",
            Some(ExecutionPhase::Running) => "正在运行…",
            None => self.status.text_for(document.id),
        };
        let cursor = self.views[self.session.active].cursor_position(cx);
        let detail = self
            .language_service
            .as_ref()
            .map(|service| {
                let (label, detail) = service.read(cx).status(document.id);
                format!(
                    "{label}{}",
                    detail
                        .map(|detail| format!("\n{detail}"))
                        .unwrap_or_default()
                )
            })
            .unwrap_or_else(|| "代码光标位置".into());
        div()
            .id("workbench-status")
            .debug_selector(|| "workbench-status".into())
            .h(px(crate::workbench_layout::STATUS_HEIGHT))
            .flex_shrink_0()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pl(px(5.))
            .pr(px(12.))
            .gap(px(16.))
            .bg(background)
            .text_color(foreground)
            .border_t_1()
            .border_color(palette.line)
            .text_size(px(11.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        crate::panel_style::button("output-toggle", "输出", cx)
                            .color(Color::Custom(foreground))
                            .label_size(LabelSize::Custom(gpui::rems(
                                px(11.) / theme::ThemeSettings::get_global(cx).ui_font_size(cx),
                            )))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_panel(
                                    crate::workbench_layout::Panel::Output,
                                    window,
                                    cx,
                                )
                            })),
                    )
                    .child(div().min_w_0().truncate().child(message.to_owned())),
            )
            .child(
                div()
                    .id("cursor-status")
                    .flex_shrink_0()
                    .child(format!("行 {}，列 {}", cursor.row + 1, cursor.column + 1))
                    .tooltip(ui::Tooltip::text(detail)),
            )
            .into_any_element()
    }
}

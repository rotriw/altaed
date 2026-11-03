use super::*;

impl Ide {
    pub(super) fn render_editor(&self, snapshot: &Session, cx: &Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_document_tabs(snapshot, cx))
            .child(
                div()
                    .id("code-editor")
                    .debug_selector(|| "code-editor".into())
                    .flex_1()
                    .min_h_0()
                    .child(self.views[self.session.active].editor.clone()),
            )
            .child(self.render_run_controls(snapshot, cx))
            .into_any_element()
    }

    fn render_run_controls(&self, snapshot: &Session, cx: &Context<Self>) -> AnyElement {
        let running = self
            .runs
            .get(&snapshot.documents[snapshot.active].id)
            .is_some_and(|run| run.running);
        let palette = crate::panel_style::palette(cx);
        div()
            .id("editor-actions")
            .debug_selector(|| "editor-actions".into())
            .flex()
            .items_center()
            .justify_end()
            .h(px(crate::workbench_layout::EDITOR_ACTIONS_HEIGHT))
            .flex_shrink_0()
            .border_t_1()
            .border_color(palette.line)
            .bg(palette.paper)
            .px(px(6.))
            .gap(px(2.))
            .child(
                crate::panel_style::command_button(
                    "compile-code",
                    "编译",
                    "编译当前代码",
                    CompileDocument,
                    cx,
                )
                .disabled(running),
            )
            .child(
                crate::panel_style::command_button(
                    "run-compiled",
                    "运行",
                    "在新终端启动上次编译成功的程序",
                    RunCompiled,
                    cx,
                )
                .disabled(running),
            )
            .child(
                crate::panel_style::command_button(
                    "run-code-tests",
                    "运行测试",
                    "编译并运行当前文档的测试",
                    RunTests,
                    cx,
                )
                .disabled(running),
            )
            .child(
                crate::panel_style::command_button(
                    "execute-code",
                    "编译并运行",
                    "编译后在新终端运行",
                    CompileAndRun,
                    cx,
                )
                .disabled(running),
            )
            .when(running, |row| {
                row.child(crate::panel_style::command_button(
                    "stop-run",
                    "停止",
                    "停止本次编译或测试",
                    CancelRun,
                    cx,
                ))
            })
            .into_any_element()
    }
}

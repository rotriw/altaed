use super::*;
use crate::runner::ExecutionPhase;
use futures::StreamExt;

#[derive(Clone, Copy, PartialEq, Eq)]
struct RunRequestId(uuid::Uuid);
use crate::{
    document::TestCase,
    runner::{self, RunMode, RunReport},
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) struct RunState {
    request: RunRequestId,
    pub phase: ExecutionPhase,
    _progress: Option<gpui::Task<()>>,
    pub stale: bool,
    pub brief: String,
    pub cancel: Arc<AtomicBool>,
    pub running: bool,
    pub report: Option<RunReport>,
    pub artifact: Option<runner::CompiledArtifact>,
    pub message: String,
}
pub(super) type Runs = HashMap<DocumentId, RunState>;

impl Ide {
    pub(super) fn mark_changed(&mut self, id: DocumentId) {
        if let Some(run) = self.runs.get_mut(&id) {
            run.stale = true;
        }
    }
    pub(super) fn start_run(&mut self, mode: RunMode, _: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.session.documents[self.session.active].clone();
        let id = snapshot.id;
        if matches!(mode, RunMode::Tests | RunMode::Test(_))
            && let Some(error) = self.checker_parameter_error(id).map(str::to_owned)
        {
            self.checker_controls.entry(id).or_default().expanded = true;
            self.layout.context_visible = true;
            self.status = StatusMessage::Document(id, format!("比较器参数未应用：{error}"));
            cx.notify();
            return;
        }
        if self.runs.get(&id).is_some_and(|run| run.running) {
            return;
        }
        let request = RunRequestId(uuid::Uuid::new_v4());
        let (progress, mut phases) = futures::channel::mpsc::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let artifact = self.runs.get(&id).and_then(|run| run.artifact.clone());
        self.runs.insert(
            id,
            RunState {
                request,
                phase: ExecutionPhase::Compiling,
                _progress: None,
                stale: false,
                brief: "正在执行…".into(),
                cancel: cancel.clone(),
                running: true,
                report: None,
                artifact,
                message: match mode {
                    RunMode::Tests | RunMode::Test(_) => "正在编译并运行测试…",
                    RunMode::Compile => "正在编译…",
                    RunMode::CompileAndRun => "正在编译并准备新终端…",
                }
                .into(),
            },
        );
        let observer = cx.spawn(async move |this, cx| {
            while let Some(phase) = phases.next().await {
                if this
                    .update(cx, |this, cx| {
                        // Queued phase events from an older execution must not
                        // recolor a subsequent run on the same document.
                        if let Some(run) = this.runs.get_mut(&id)
                            && run.request == request
                            && run.running
                        {
                            run.phase = phase;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        if let Some(run) = self.runs.get_mut(&id) {
            run._progress = Some(observer);
        }
        self.layout.context_visible = true;
        self.layout.messages_visible = true;
        let root = self.directory.join("runs");
        let compiler = self.preferences.toolchain.compiler_path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let report = runner::execute_with_progress(
                        snapshot,
                        mode,
                        &root,
                        cancel,
                        compiler,
                        |phase| {
                            let _ = progress.unbounded_send(phase);
                        },
                    )?;
                    let terminal = if mode == RunMode::CompileAndRun {
                        let _ = progress.unbounded_send(ExecutionPhase::Running);
                        report
                            .artifact()
                            .map(|artifact| runner::run_compiled(&artifact, &root))
                    } else {
                        None
                    };
                    anyhow::Ok((report, terminal))
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some(run) = this.runs.get_mut(&id) {
                    run.running = false;
                    match result {
                        Ok((report, terminal)) => {
                            if let Some(artifact) = report.artifact() {
                                run.artifact = Some(artifact);
                            }
                            run.stale |= this
                                .session
                                .documents
                                .iter()
                                .find(|d| d.id == id)
                                .is_none_or(|d| !report.is_current(d));
                            run.brief = report.brief();
                            run.message = report.summary();
                            if let Some(terminal) = terminal {
                                let message = match terminal {
                                    Ok(id) => format!("已请求在新终端运行 · 会话 {id}"),
                                    Err(error) => format!("编译成功；终端启动失败：{error:#}"),
                                };
                                run.brief.push_str(&format!("\n{message}"));
                                run.message.push_str(&format!("\n{message}"));
                            }
                            run.report = Some(report);
                        }
                        Err(error) => run.message = format!("执行未完成：{error:#}"),
                    }
                }
                if let Some(run) = this.runs.get_mut(&id)
                    && run.report.is_none()
                {
                    run.brief = run.message.chars().take(2000).collect();
                }
                cx.notify();
            })
        })
        .detach_and_log_err(cx);
        cx.notify();
    }
    pub(super) fn run_compiled(&mut self, _: &RunCompiled, _: &mut Window, cx: &mut Context<Self>) {
        let document = &self.session.documents[self.session.active];
        let id = document.id;
        let Some(run) = self.runs.get_mut(&id) else {
            self.status = StatusMessage::Document(id, "尚无编译产物，请先编译当前代码".into());
            cx.notify();
            return;
        };
        if run.running {
            return;
        }
        let Some(artifact) = run.artifact.clone() else {
            self.status =
                StatusMessage::Document(id, "尚无成功的编译产物，请先编译当前代码".into());
            cx.notify();
            return;
        };
        let selected_compiler = document.metadata.settings.compiler_path.as_ref().or(self
            .preferences
            .toolchain
            .compiler_path
            .as_ref());
        let current = artifact.is_current(document)
            && selected_compiler.is_none_or(|compiler| compiler == &artifact.compiler);
        run.running = true;
        run.request = RunRequestId(uuid::Uuid::new_v4());
        run.phase = ExecutionPhase::Running;
        run.brief = "正在启动已有程序…".into();
        let root = self.directory.join("runs");
        cx.spawn(async move |this, cx| {
            let build = artifact.build_id;
            let compiler = artifact.compiler.clone();
            let result = cx
                .background_spawn(async move { runner::run_compiled(&artifact, &root) })
                .await;
            this.update(cx, |this, cx| {
                if let Some(run) = this.runs.get_mut(&id) {
                    run.running = false;
                    run.brief = match result {
                        Ok(launch) => format!(
                            "已请求在新终端运行{}\n构建 {build} · 会话 {launch}\n编译器：{}",
                            if current {
                                ""
                            } else {
                                "上次编译的版本；当前代码或配置已有修改"
                            },
                            compiler.display()
                        ),
                        Err(error) => format!("终端未启动：{error:#}"),
                    };
                    run.message = run.brief.clone();
                }
                cx.notify();
            })
        })
        .detach_and_log_err(cx);
        self.layout.context_visible = true;
        self.layout.messages_visible = true;
        cx.notify();
    }
    pub(super) fn cancel_run(&mut self, _: &CancelRun, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(run) = self
            .runs
            .get(&self.session.documents[self.session.active].id)
        {
            run.cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }
    pub(super) fn edit_configuration(
        &mut self,
        _: &ConfigureRun,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_configuration(window, cx);
    }
    pub(super) fn add_test(&mut self, _: &AddTest, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        let doc = &mut self.session.documents[self.session.active];
        doc.metadata.tests.push(TestCase {
            input: String::new(),
            expected_output: Some(String::new()),
            ..Default::default()
        });
        let id = doc.id;
        self.test_editors.remove(&id);
        self.mark_changed(id);
        self.layout.context_visible = true;
        self.layout.context_tab = crate::workbench_layout::ContextTab::Tests;
        cx.notify();
    }
    fn open_configuration(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.show_settings(settings_window::Scope::Document, cx);
    }
}

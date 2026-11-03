pub(crate) mod process;
pub use crate::toolchain::{discover_compiler, verify_compiler};
use crate::{checker::Checker, document::CodeDocument};
use anyhow::{Context, Result, ensure};
pub use process::{ProcessOutput, Termination};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutionSettings {
    pub compile_timeout_ms: u64,
    pub test_timeout_ms: u64,
    pub output_limit_bytes: u64,
    pub checker: Checker,
}
impl Default for ExecutionSettings {
    fn default() -> Self {
        Self {
            compile_timeout_ms: 30_000,
            test_timeout_ms: 2_000,
            output_limit_bytes: 1_048_576,
            checker: Checker::default(),
        }
    }
}
impl ExecutionSettings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.compile_timeout_ms > 0 && self.test_timeout_ms > 0 && self.output_limit_bytes > 0,
            "超时和输出上限必须大于零"
        );
        self.checker.validate()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Compile,
    Tests,
    Test(usize),
    CompileAndRun,
}
#[derive(Debug)]
pub enum Verdict {
    Passed,
    WrongAnswer,
    Observed,
    RuntimeError,
    TimedOut,
    Cancelled,
    OutputLimit,
}
pub struct CaseResult {
    pub index: usize,
    pub output: ProcessOutput,
    pub verdict: Verdict,
}
pub struct RunReport {
    pub id: Uuid,
    pub snapshot: CodeDocument,
    pub directory: PathBuf,
    pub executable: PathBuf,
    pub compiler: PathBuf,
    pub working_directory: PathBuf,
    pub compile: ProcessOutput,
    pub cases: Vec<CaseResult>,
    pub mode: RunMode,
}
impl RunReport {
    pub fn is_current(&self, document: &CodeDocument) -> bool {
        self.snapshot.id == document.id
            && self.snapshot.text == document.text
            && self.snapshot.metadata == document.metadata
    }
    pub fn compiled(&self) -> bool {
        self.compile.termination == Termination::Exited && self.compile.status.success()
    }
    pub fn brief(&self) -> String {
        let headline = if self.compiled() {
            "编译完成"
        } else {
            match self.compile.termination {
                Termination::Cancelled => "编译已取消",
                Termination::TimedOut => "编译超时",
                Termination::OutputLimit => "编译输出超限",
                Termination::Exited => "编译失败",
            }
        };
        let mut text = format!("{headline} · {} ms\n", self.compile.elapsed.as_millis());
        if self.compiled() {
            match self.mode {
                RunMode::Compile => text.push_str("已生成独立可执行文件。路径见详细输出。\n"),
                RunMode::CompileAndRun => text.push_str("程序已编译；终端启动状态见运行信息。\n"),
                _ => {
                    for case in &self.cases {
                        text.push_str(&format!(
                            "用例 {}：{} · {} ms\n",
                            case.index + 1,
                            case.verdict.label(),
                            case.output.elapsed.as_millis()
                        ));
                        if text.len() > 1000 {
                            text.push_str("其余结果见各用例。\n");
                            break;
                        }
                    }
                }
            }
        }
        let diagnostics = String::from_utf8_lossy(&self.compile.stderr);
        text.extend(diagnostics.chars().take(1200));
        text
    }
    pub fn summary(&self) -> String {
        let mut text = format!(
            "执行 {} · {:?}\n目录：{}\n编译器：{}\n编译：{:?} · {} · {} ms\n{}{}",
            self.id,
            self.mode,
            self.directory.display(),
            self.compiler.display(),
            self.compile.termination,
            self.compile.status,
            self.compile.elapsed.as_millis(),
            String::from_utf8_lossy(&self.compile.stdout),
            String::from_utf8_lossy(&self.compile.stderr)
        );
        if self.compiled() {
            text.push_str(&format!("\n产物：{}\n", self.executable.display()));
        }
        for case in &self.cases {
            text.push_str(&format!(
                "\n测试 {}：{} · {} ms\n实际输出：\n{}\n标准错误：\n{}\n",
                case.index + 1,
                case.verdict.label(),
                case.output.elapsed.as_millis(),
                String::from_utf8_lossy(&case.output.stdout),
                String::from_utf8_lossy(&case.output.stderr)
            ));
        }
        const DISPLAY_LIMIT: usize = 65_536;
        if text.len() > DISPLAY_LIMIT {
            let mut end = DISPLAY_LIMIT;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str("\n显示内容已截断；完整输出保存在本次执行目录。\n");
        }
        text
    }
}
impl Verdict {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passed => "通过",
            Self::WrongAnswer => "答案不符",
            Self::Observed => "运行结束（未设置预期输出）",
            Self::RuntimeError => "运行错误",
            Self::TimedOut => "超时",
            Self::Cancelled => "已取消",
            Self::OutputLimit => "输出超限",
        }
    }
}

pub fn execute(
    snapshot: CodeDocument,
    mode: RunMode,
    root: &Path,
    cancel: Arc<AtomicBool>,
    default_compiler: Option<PathBuf>,
) -> Result<RunReport> {
    let settings = &snapshot.metadata.settings;
    settings.execution.validate()?;
    if matches!(mode, RunMode::Tests | RunMode::Test(_)) {
        ensure!(
            !snapshot.metadata.tests.is_empty(),
            "当前文档没有测试，请先添加测试"
        );
    }
    let requested_compiler = settings
        .compiler_path
        .clone()
        .or(default_compiler)
        .or_else(discover_compiler)
        .context("未找到 C++ 编译器，请在“运行配置”中指定路径")?;
    // Freeze the resolved driver for this build and its later terminal runs.
    // A bare PATH command cannot locate a bundled MinGW runtime on its own.
    let compiler = crate::toolchain::find_executable(&requested_compiler)
        .with_context(|| format!("找不到 C++ 编译器：{}", requested_compiler.display()))?;
    let working = settings
        .working_directory
        .clone()
        .or_else(|| {
            snapshot
                .path
                .as_ref()
                .and_then(|p| p.parent().map(Path::to_owned))
        })
        .unwrap_or(std::env::current_dir()?);
    let working = fs::canonicalize(working).context("工作目录不可用")?;
    let id = Uuid::new_v4();
    let directory = root.join(id.to_string());
    fs::create_dir_all(&directory)?;
    let directory = fs::canonicalize(directory)?;
    let source = directory.join("main.cpp");
    fs::write(&source, &snapshot.text)?;
    let executable = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let mut command = Command::new(&compiler);
    command
        .current_dir(&working)
        .args(&settings.compiler_arguments)
        .arg(&source)
        .arg("-o")
        .arg(&executable);
    if let Some(parent) = snapshot.path.as_ref().and_then(|p| p.parent()) {
        command.arg("-iquote").arg(parent);
    }
    let compile = process::execute(
        command,
        b"",
        &directory.join("compile"),
        Duration::from_millis(settings.execution.compile_timeout_ms),
        settings.execution.output_limit_bytes,
        &cancel,
    )?;
    let mut cases = Vec::new();
    if compile.termination == Termination::Exited
        && compile.status.success()
        && matches!(mode, RunMode::Tests | RunMode::Test(_))
    {
        for (index, test) in snapshot.metadata.tests.iter().enumerate() {
            if matches!(mode, RunMode::Test(selected) if selected != index) {
                continue;
            }
            let mut command = crate::toolchain::runtime_command(&executable, &compiler)?;
            command.current_dir(&working);
            let output = process::execute(
                command,
                test.input.as_bytes(),
                &directory.join(format!("case-{index}")),
                Duration::from_millis(settings.execution.test_timeout_ms),
                settings.execution.output_limit_bytes,
                &cancel,
            )?;
            let verdict = match output.termination {
                Termination::TimedOut => Verdict::TimedOut,
                Termination::Cancelled => Verdict::Cancelled,
                Termination::OutputLimit => Verdict::OutputLimit,
                Termination::Exited if !output.status.success() => Verdict::RuntimeError,
                _ => match &test.expected_output {
                    None => Verdict::Observed,
                    Some(expected)
                        if settings.execution.checker.accepts(expected, &output.stdout) =>
                    {
                        Verdict::Passed
                    }
                    _ => Verdict::WrongAnswer,
                },
            };
            let cancelled = output.termination == Termination::Cancelled;
            cases.push(CaseResult {
                index,
                output,
                verdict,
            });
            if cancelled {
                break;
            }
        }
    }
    Ok(RunReport {
        id,
        snapshot,
        directory,
        executable,
        compiler,
        working_directory: working,
        compile,
        cases,
        mode,
    })
}

/// A successful immutable build survives later failures. It belongs to one
/// document and retains the source and working directory used by that build.
#[derive(Clone)]
pub struct CompiledArtifact {
    pub build_id: Uuid,
    pub snapshot: CodeDocument,
    pub executable: PathBuf,
    pub compiler: PathBuf,
    pub working_directory: PathBuf,
}
impl RunReport {
    pub fn artifact(&self) -> Option<CompiledArtifact> {
        self.compiled().then(|| CompiledArtifact {
            build_id: self.id,
            snapshot: self.snapshot.clone(),
            executable: self.executable.clone(),
            compiler: self.compiler.clone(),
            working_directory: self.working_directory.clone(),
        })
    }
}
impl CompiledArtifact {
    pub fn is_current(&self, document: &CodeDocument) -> bool {
        self.snapshot.id == document.id
            && self.snapshot.text == document.text
            && self.snapshot.path == document.path
            && self.snapshot.metadata.settings == document.metadata.settings
    }
}
/// Every launch gets a separate script. Never rewrite the script or executable
/// referenced by an already open terminal session.
pub fn run_compiled(artifact: &CompiledArtifact, root: &Path) -> Result<Uuid> {
    ensure!(
        artifact.executable.is_file(),
        "编译产物已不存在，请重新编译"
    );
    let id = Uuid::new_v4();
    let directory = root.join(id.to_string());
    fs::create_dir_all(&directory).context("无法创建终端会话目录")?;
    crate::platform::terminal::launch(
        &artifact.executable,
        &artifact.compiler,
        &artifact.working_directory,
        &directory,
    )?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::TestCase;
    use std::sync::atomic::Ordering;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("altaed runner test {}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn real_compiler_unsaved_snapshot_and_test_failures() -> Result<()> {
        let root = Scratch::new();
        let compiler = discover_compiler().context("真实编译回归需要 PATH 中有 C++ 编译器")?;
        fs::write(root.0.join("value.h"), "constexpr int answer = 42;\n")?;
        let original = root.0.join("code.cpp");
        fs::write(&original, "disk version")?;
        let mut doc = CodeDocument::untitled();
        doc.path = Some(original.clone());
        doc.metadata.settings.compiler_path = Some(compiler.clone());
        doc.metadata.settings.execution.test_timeout_ms = 150;
        doc.metadata.settings.execution.output_limit_bytes = 65_536;
        doc.text = "#include <iostream>\n#include \"value.h\"\nint main(){int n;std::cin>>n;if(n==1)while(true){};if(n==2){while(true)std::cout<<\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\";} if(n==3)return 7;std::cout<<answer;}".into();
        doc.metadata.tests = vec![
            TestCase {
                input: "0".into(),
                expected_output: Some("42\n".into()),
            },
            TestCase {
                input: "0".into(),
                expected_output: Some("43".into()),
            },
            TestCase {
                input: "0".into(),
                expected_output: None,
            },
            TestCase {
                input: "1".into(),
                expected_output: None,
            },
            TestCase {
                input: "2".into(),
                expected_output: None,
            },
            TestCase {
                input: "3".into(),
                expected_output: None,
            },
            TestCase {
                input: "0".into(),
                expected_output: Some("".into()),
            },
        ];
        let cancel = Arc::new(AtomicBool::new(false));
        let report = execute(
            doc.clone(),
            RunMode::Tests,
            &root.0.join("runs"),
            cancel.clone(),
            None,
        )?;
        assert!(report.compiled(), "{}", report.summary());
        let labels: Vec<_> = report.cases.iter().map(|c| c.verdict.label()).collect();
        assert_eq!(
            labels,
            [
                "通过",
                "答案不符",
                "运行结束（未设置预期输出）",
                "超时",
                "输出超限",
                "运行错误",
                "答案不符"
            ]
        );
        let selected = execute(
            doc.clone(),
            RunMode::Test(1),
            &root.0.join("runs"),
            cancel.clone(),
            None,
        )?;
        assert_eq!(selected.cases.len(), 1);
        assert_eq!(selected.cases[0].index, 1);
        assert!(matches!(selected.cases[0].verdict, Verdict::WrongAnswer));
        assert_eq!(fs::read_to_string(original)?, "disk version");
        doc.text.push_str("\n// edit after execution");
        assert!(!report.is_current(&doc));
        doc.path = None;
        doc.text = "int main(){ broken source }".into();
        let failed = execute(
            doc.clone(),
            RunMode::Tests,
            &root.0.join("runs"),
            cancel.clone(),
            None,
        )?;
        assert!(!failed.compiled());
        assert!(failed.cases.is_empty());
        assert_ne!(failed.directory, report.directory);
        doc.text = "int main(){}".into();
        doc.metadata.tests.clear();
        let unsaved = execute(
            doc.clone(),
            RunMode::Compile,
            &root.0.join("runs"),
            cancel.clone(),
            None,
        )?;
        assert!(unsaved.compiled());
        assert!(unsaved.executable.is_file());
        let artifact = unsaved.artifact().context("成功编译必须生成产物")?;
        assert!(artifact.is_current(&doc));
        let bytes = fs::read(&artifact.executable)?;
        doc.text = "int main(){ invalid source }".into();
        let failed = execute(
            doc.clone(),
            RunMode::CompileAndRun,
            &root.0.join("runs"),
            cancel.clone(),
            None,
        )?;
        assert!(failed.artifact().is_none());
        assert!(!artifact.is_current(&doc));
        assert_eq!(fs::read(&artifact.executable)?, bytes);
        assert!(Command::new(&artifact.executable).status()?.success());
        let other = doc.fork(root.0.join("other.cpp"));
        assert!(!artifact.is_current(&other));
        fs::remove_file(&artifact.executable)?;
        assert!(run_compiled(&artifact, &root.0.join("runs")).is_err());
        doc.text = "int main(){}".into();
        cancel.store(true, Ordering::Relaxed);
        let cancelled = execute(doc, RunMode::Compile, &root.0.join("runs"), cancel, None)?;
        assert_eq!(cancelled.compile.termination, Termination::Cancelled);
        Ok(())
    }
}

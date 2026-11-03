//! Freeze the selected native checker once per execution, then evaluate each
//! case against its own immutable files using the same bounded process adapter.
use super::{CheckerOutput, Termination, Verdict, process};
use crate::{checker::ExecutableChecker, document::CodeDocument};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

// Bound the copy independently of sample/output limits: debug executables can
// legitimately be much larger than the test data they inspect.
const MAX_CHECKER_BYTES: u64 = 256 * 1024 * 1024;
pub(super) struct PreparedChecker {
    program: PathBuf,
    original_directory: PathBuf,
    settings: ExecutableChecker,
}
pub(super) fn prepare(
    document: &CodeDocument,
    settings: &ExecutableChecker,
    directory: &Path,
) -> Result<PreparedChecker> {
    let original =
        ExecutableChecker::validate_program(&document.resolve_attachment_path(&settings.program)?)?;
    let mut source = fs::File::open(&original).context("无法读取比较器快照")?;
    let before = source.metadata()?;
    ensure!(
        before.len() <= MAX_CHECKER_BYTES,
        "比较器超过 256 MiB 快照上限"
    );
    let target = directory.join("checker");
    fs::create_dir_all(&target)?;
    let program = target.join(original.file_name().context("比较器路径缺少文件名")?);
    let mut destination = fs::File::create(&program)?;
    let bytes = std::io::copy(
        &mut (&mut source).take(MAX_CHECKER_BYTES + 1),
        &mut destination,
    )?;
    let after = source.metadata()?;
    ensure!(
        bytes <= MAX_CHECKER_BYTES
            && bytes == before.len()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "比较器在读取时发生变化，请重试"
    );
    fs::set_permissions(&program, before.permissions())?;
    Ok(PreparedChecker {
        program,
        original_directory: original.parent().context("比较器路径缺少目录")?.to_owned(),
        settings: settings.clone(),
    })
}
pub(super) fn evaluate(
    checker: &PreparedChecker,
    case: &Path,
    compiler: &Path,
    limit: u64,
    cancel: &Arc<AtomicBool>,
) -> (Verdict, CheckerOutput) {
    let result = (|| {
        let mut command = crate::toolchain::checker_command(
            &checker.program,
            compiler,
            &checker.original_directory,
        )?;
        // Testlib registerTestlibCmd: input, contestant output, jury answer.
        command
            .current_dir(case)
            .arg(case.join("stdin"))
            .arg(case.join("stdout"))
            .arg(case.join("expected"))
            .args(&checker.settings.arguments);
        process::execute(
            command,
            b"",
            &case.join("checker"),
            Duration::from_millis(checker.settings.timeout_ms),
            limit,
            cancel,
        )
        .context("无法运行比较器")
    })();
    match result {
        Ok(output) => {
            let verdict = match output.termination {
                Termination::Cancelled => Verdict::Cancelled,
                Termination::Exited => match output.status.code() {
                    Some(0) => Verdict::Passed,
                    // Standard testlib WA, PE, extra output, unexpected EOF.
                    Some(1 | 2 | 4 | 8) => Verdict::WrongAnswer,
                    _ => Verdict::CheckerError,
                },
                _ => Verdict::CheckerError,
            };
            (verdict, CheckerOutput::Completed(output))
        }
        Err(error) => (
            Verdict::CheckerError,
            CheckerOutput::Failed(format!("{error:#}")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        checker::Checker,
        document::TestCase,
        runner::{RunMode, execute},
        toolchain::discover_compiler,
    };
    use std::sync::atomic::Ordering;
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn native_checker_uses_snapshots_and_distinguishes_failures() -> Result<()> {
        let root =
            Scratch(std::env::temp_dir().join(format!("altaed checker {}", uuid::Uuid::new_v4())));
        fs::create_dir_all(&root.0)?;
        let compiler = discover_compiler().context("比较器回归需要真实 C++ 编译器")?;
        let source = root.0.join("checker.cpp");
        let program = root
            .0
            .join(format!("custom checker{}", std::env::consts::EXE_SUFFIX));
        fs::write(
            &source,
            r#"
#include <fstream>
#include <iostream>
#include <string>
int main(int argc,char** argv){
 if(argc!=5 || std::string(argv[4])!="an argument with spaces") return 3;
 long long input=0,actual=0; int code=3;
 if(!(std::ifstream(argv[1])>>input) || !(std::ifstream(argv[2])>>actual) || !(std::ifstream(argv[3])>>code)) return 3;
 if(actual!=input*2) return 3;
 std::cerr<<"checker diagnosis "<<code;
 if(code==9) while(true){}
 return code;
}
"#,
        )?;
        let compiled = std::process::Command::new(&compiler)
            .arg(&source)
            .arg("-o")
            .arg(&program)
            .output()?;
        ensure!(
            compiled.status.success(),
            "checker fixture failed: {}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let mut document = CodeDocument::untitled();
        document.metadata.settings.compiler_path = Some(compiler.clone());
        document.text =
            "#include <iostream>\nint main(){long long n;std::cin>>n;std::cout<<n*2;}".into();
        let settings = ExecutableChecker {
            program: program.clone(),
            timeout_ms: 100,
            arguments: vec!["an argument with spaces".into()],
        };
        document.metadata.settings.execution.checker = Checker::Executable(settings.clone());
        document.metadata.tests = [0, 1, 2, 3, 4, 8, 7, 9]
            .into_iter()
            .map(|code| TestCase {
                input: "21".into(),
                expected_output: Some(code.to_string()),
                ..Default::default()
            })
            .chain(std::iter::once(TestCase {
                input: "21".into(),
                ..Default::default()
            }))
            .collect();
        let cancel = Arc::new(AtomicBool::new(false));
        let report = execute(
            document.clone(),
            RunMode::Tests,
            &root.0.join("runs"),
            cancel.clone(),
            None,
        )?;
        assert!(report.compiled(), "{}", report.summary());
        let labels: Vec<_> = report
            .cases
            .iter()
            .map(|case| case.verdict.label())
            .collect();
        assert_eq!(
            labels,
            [
                "Accepted",
                "WrongAnswer",
                "WrongAnswer",
                "比较器错误",
                "WrongAnswer",
                "WrongAnswer",
                "比较器错误",
                "比较器错误",
                "运行结束（未设置预期输出）"
            ]
        );
        assert!(report.summary().contains("checker diagnosis 3"));
        assert_eq!(report.cases[0].output.stdout, b"42");
        assert!(report.cases[8].checker.is_none());
        let prepared = prepare(&document, &settings, &root.0.join("frozen"))?;
        fs::write(&program, b"replaced after snapshot")?;
        let (verdict, _) = evaluate(
            &prepared,
            &report.directory.join("case-0"),
            &compiler,
            4096,
            &cancel,
        );
        assert!(matches!(verdict, Verdict::Passed));
        cancel.store(true, Ordering::Relaxed);
        let (verdict, _) = evaluate(
            &prepared,
            &report.directory.join("case-0"),
            &compiler,
            4096,
            &cancel,
        );
        assert!(matches!(verdict, Verdict::Cancelled));
        assert!(ExecutableChecker::validate_program(&source).is_err());
        assert!(ExecutableChecker::validate_program(&root.0.join("missing")).is_err());
        Ok(())
    }
}

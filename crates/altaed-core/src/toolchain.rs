//! Local toolchain discovery and first-run verification, without a UI context.
use crate::runner::{Termination, process};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use uuid::Uuid;

pub fn discover_compiler() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    if let Some(bundled) = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .map(|root| root.join("toolchains/mingw64/bin/g++.exe"))
        })
        .filter(|path| path.is_file())
    {
        return Some(bundled);
    }
    ["clang++", "g++", "c++"]
        .into_iter()
        .find_map(|name| find_executable(Path::new(name)))
}
pub fn find_executable(path: &Path) -> Option<PathBuf> {
    find_in_paths(
        path,
        std::env::var_os("PATH").as_deref(),
        std::env::consts::EXE_SUFFIX,
        &std::env::current_dir().ok()?,
    )
}

fn find_in_paths(
    path: &Path,
    search: Option<&std::ffi::OsStr>,
    suffix: &str,
    current: &Path,
) -> Option<PathBuf> {
    let mut names = vec![path.to_owned()];
    if !suffix.is_empty()
        && !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(&suffix[1..]))
    {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        names.push(name.into());
    }
    if path.is_absolute() || path.components().count() > 1 {
        return names
            .into_iter()
            .map(|path| current.join(path))
            .find(|path| path.is_file());
    }
    std::env::split_paths(search?)
        .flat_map(|directory| {
            names
                .iter()
                .map(move |name| current.join(&directory).join(name))
        })
        .find(|candidate| candidate.is_file())
}

/// MinGW executables can depend on DLLs beside their selected driver. Change
/// only the child's PATH; never mutate the IDE's process-wide environment.
pub(crate) fn runtime_command(executable: &Path, compiler: &Path) -> Result<Command> {
    #[allow(unused_mut)]
    let mut command = Command::new(executable);
    #[cfg(target_os = "windows")]
    {
        let bin = compiler.parent().context("编译器路径没有父目录")?;
        let mut paths = vec![bin.to_owned()];
        if let Some(search) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&search));
        }
        command.env(
            "PATH",
            std::env::join_paths(paths).context("无法设置运行时 DLL 搜索路径")?,
        );
    }
    #[cfg(not(target_os = "windows"))]
    let _ = compiler;
    Ok(command)
}
/// Compile and execute a C++20 program with standard-library I/O. A compiler
/// shim or a missing SDK must not be reported as a usable environment.
pub fn verify_compiler(path: &Path, root: &Path, cancel: Arc<AtomicBool>) -> Result<PathBuf> {
    let compiler =
        find_executable(path).context("找不到指定的编译器，请填写完整路径或 PATH 中的命令")?;
    let directory = root.join(Uuid::new_v4().to_string());
    fs::create_dir_all(&directory).context("无法创建环境检测目录")?;
    let result = verify_in(&compiler, &directory, &cancel);
    let cleanup = fs::remove_dir_all(&directory);
    let verified = result?;
    cleanup.context("检测已完成，但临时文件清理失败")?;
    Ok(verified)
}
fn verify_in(compiler: &Path, directory: &Path, cancel: &Arc<AtomicBool>) -> Result<PathBuf> {
    let directory = fs::canonicalize(directory)?;
    let source = directory.join("verify.cpp");
    let executable = directory.join(format!("verify{}", std::env::consts::EXE_SUFFIX));
    fs::write(
        &source,
        "#include <iostream>\nint main() { std::cout << __cplusplus; }\n",
    )?;
    let mut command = Command::new(compiler);
    command
        .args(["-std=c++20"])
        .arg(&source)
        .arg("-o")
        .arg(&executable);
    let output = process::execute(
        command,
        b"",
        &directory.join("compile"),
        Duration::from_secs(30),
        65_536,
        cancel,
    )?;
    ensure!(
        output.termination == Termination::Exited && output.status.success(),
        "C++20 编译检测失败（{:?}）：{}",
        output.termination,
        String::from_utf8_lossy(&output.stderr)
    );
    let output = process::execute(
        runtime_command(&executable, compiler)?,
        b"",
        &directory.join("run"),
        Duration::from_secs(3),
        4096,
        cancel,
    )?;
    ensure!(
        output.termination == Termination::Exited
            && output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u64>()
                .is_ok_and(|version| version >= 202002),
        "程序执行检测失败，请检查工具链和系统运行环境"
    );
    // Keep the selected driver name: resolving a clang++ symlink to clang can
    // change its language/runtime-linking behavior on subsequent invocations.
    Ok(compiler.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn executable_lookup_accepts_windows_suffix_and_relative_path_entries() -> Result<()> {
        let root = std::env::temp_dir().join(format!("altaed lookup {}", Uuid::new_v4()));
        fs::create_dir_all(root.join("工具链/bin"))?;
        let driver = root.join("工具链/bin/g++.exe");
        fs::write(&driver, b"lookup fixture, never executed")?;
        let search = std::env::join_paths([Path::new("工具链/bin")])?;
        assert_eq!(
            find_in_paths(Path::new("g++"), Some(&search), ".exe", &root),
            Some(driver.clone())
        );
        assert_eq!(
            find_in_paths(Path::new("g++.exe"), Some(&search), ".exe", &root),
            Some(driver.clone())
        );
        assert_eq!(
            find_in_paths(Path::new("工具链/bin/g++"), None, ".exe", &root),
            Some(driver)
        );
        assert!(find_in_paths(Path::new("missing"), Some(&search), ".exe", &root).is_none());
        fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn verifies_real_cpp20_environment_and_rejects_missing_driver() -> Result<()> {
        let root = std::env::temp_dir().join(format!("altaed-toolchain-{}", Uuid::new_v4()));
        let compiler = discover_compiler().context("工具链回归需要真实 C++ 编译器")?;
        assert_eq!(
            verify_compiler(&compiler, &root, Arc::new(AtomicBool::new(false)))?,
            compiler
        );
        assert_eq!(fs::read_dir(&root)?.count(), 0);
        assert!(
            verify_compiler(
                &root.join("missing-compiler"),
                &root,
                Arc::new(AtomicBool::new(false))
            )
            .is_err()
        );
        fs::remove_dir(root)?;
        Ok(())
    }
}

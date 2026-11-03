//! Terminal adapters consume immutable build paths. Each launch has its own
//! script; a later compile never rewrites a running session's entry point.
use anyhow::{Context, Result};
use std::{fs, path::Path, process::Command};

pub fn launch(executable: &Path, compiler: &Path, working: &Path, directory: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = compiler;
        let quote = |path: &Path| -> Result<String> {
            let value = path.to_str().context("终端路径不是有效 UTF-8")?;
            Ok(format!("'{}'", value.replace('\'', "'\\''")))
        };
        let script = directory.join("run.command");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\ncd -- {} || exit 1\nexec {}\n",
                quote(working)?,
                quote(executable)?
            ),
        )?;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))?;
        anyhow::ensure!(
            Command::new("open")
                .args(["-a", "Terminal"])
                .arg(script)
                .status()?
                .success(),
            "无法打开系统终端"
        );
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        use base64::Engine;
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        let script = directory.join("run.ps1");
        let commands = powershell_script(executable, compiler, working)?;
        // Windows PowerShell 5.1 needs a BOM to read non-ASCII paths as UTF-8.
        fs::write(&script, format!("\u{feff}{commands}"))?;
        // PowerShell's documented UTF-16LE transport preserves Unicode and
        // quoting without changing the user's script execution policy.
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            commands
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NoExit", "-EncodedCommand"])
            .arg(encoded)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .context("无法打开 PowerShell 终端")?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (executable, compiler, working, directory);
        anyhow::bail!("当前平台的终端启动器尚未实现")
    }
}

#[cfg(any(target_os = "windows", test))]
fn powershell_script(executable: &Path, compiler: &Path, working: &Path) -> Result<String> {
    let literal = |path: &Path| -> Result<String> {
        Ok(format!(
            "'{}'",
            path.to_str()
                .context("终端路径不是有效 Unicode")?
                .replace('\'', "''")
        ))
    };
    let bin = compiler.parent().context("编译器路径没有父目录")?;
    Ok(format!(
        "$ErrorActionPreference = 'Stop'\r\nSet-Location -LiteralPath {}\r\n$env:PATH = {} + ';' + $env:PATH\r\n& {}\r\n",
        literal(working)?,
        literal(bin)?,
        literal(executable)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_paths_are_literal_and_keep_unicode_and_shell_characters() -> Result<()> {
        let script = powershell_script(
            Path::new("build O'Brien/$answer & (1)/main.exe"),
            Path::new("工具链/bin/g++.exe"),
            Path::new("题目 O'Brien/$data & (1)"),
        )?;
        assert!(script.contains("Set-Location -LiteralPath '题目 O''Brien/$data & (1)'"));
        assert!(script.contains("& 'build O''Brien/$answer & (1)/main.exe'"));
        assert!(script.contains("$env:PATH = '工具链/bin' + ';' + $env:PATH"));
        Ok(())
    }
}

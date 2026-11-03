use anyhow::{Context, Result};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    process::{Command, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    Exited,
    TimedOut,
    Cancelled,
    OutputLimit,
}
#[derive(Debug)]
pub struct ProcessOutput {
    pub status: ExitStatus,
    pub termination: Termination,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed: Duration,
}

pub fn execute(
    mut command: Command,
    input: &[u8],
    directory: &Path,
    timeout: Duration,
    limit: u64,
    cancel: &Arc<AtomicBool>,
) -> Result<ProcessOutput> {
    let input_path = directory.join("stdin");
    let output_path = directory.join("stdout");
    let error_path = directory.join("stderr");
    fs::create_dir_all(directory)?;
    fs::write(&input_path, input)?;
    command
        .stdin(File::open(input_path)?)
        .stdout(File::create(&output_path)?)
        .stderr(File::create(&error_path)?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Batch compilation/tests already redirect all streams. Their console
        // children must not flash a window for every case; interactive launches
        // use the separate terminal adapter with CREATE_NEW_CONSOLE instead.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let start = Instant::now();
    let mut child = command
        .spawn()
        .context("无法启动进程，请检查编译器路径与运行配置")?;
    let result: Result<(Termination, Option<ExitStatus>)> = (|| {
        loop {
            let termination = if cancel.load(Ordering::Relaxed) {
                Some(Termination::Cancelled)
            } else if start.elapsed() >= timeout {
                Some(Termination::TimedOut)
            } else if fs::metadata(&output_path)?
                .len()
                .saturating_add(fs::metadata(&error_path)?.len())
                > limit
            {
                Some(Termination::OutputLimit)
            } else {
                None
            };
            if let Some(termination) = termination {
                return Ok((termination, None));
            }
            if let Some(status) = child.try_wait()? {
                // A fast process can exceed the limit and exit between polls.
                let termination = if fs::metadata(&output_path)?
                    .len()
                    .saturating_add(fs::metadata(&error_path)?.len())
                    > limit
                {
                    Termination::OutputLimit
                } else {
                    Termination::Exited
                };
                return Ok((termination, Some(status)));
            }
            thread::sleep(Duration::from_millis(10));
        }
    })();
    // Clean up children in the same group, including children left by an exited parent.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let waited = child.wait();
    let (termination, status) = result?;
    let read = |path: &Path| -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        File::open(path)?.take(limit).read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    Ok(ProcessOutput {
        status: status.map(Ok).unwrap_or(waited)?,
        termination,
        stdout: read(&output_path)?,
        stderr: read(&error_path)?,
        elapsed: start.elapsed(),
    })
}

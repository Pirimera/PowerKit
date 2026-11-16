//! Spawn Scoop/Chocolatey CLI children off the UI thread with cancel-by-kill.
use super::broker::CancelHandle;
use super::winget::WinGetError;
use crate::power_store::{OperationProgress, ProgressStage};
use std::io::{BufReader, Read};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Resolve `name` on PATH, returning the first existing file path.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions = ["", ".exe", ".cmd", ".bat", ".ps1"];
    for dir in std::env::split_paths(&path) {
        for extension in extensions {
            let candidate = dir.join(format!("{name}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Run a CLI tool to completion, collecting stdout. Cancel kills the process.
pub fn run_collect(
    program: &Path,
    args: &[&str],
    cancel: Option<&CancelHandle>,
    on_progress: Option<&mut dyn FnMut(OperationProgress)>,
) -> Result<String, WinGetError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(|error| {
        WinGetError::Operation(format!("{} could not start: {error}", program.display()))
    })?;
    if let Some(cancel) = cancel {
        cancel.attach_pid(child.id());
    }
    if let Some(on_progress) = on_progress {
        on_progress(OperationProgress {
            stage: ProgressStage::Installing,
            percent: None,
        });
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let output_handle = thread::spawn(move || {
        let mut text = String::new();
        if let Some(stdout) = stdout {
            let _ = BufReader::new(stdout).read_to_string(&mut text);
        }
        let mut err = String::new();
        if let Some(stderr) = stderr {
            let _ = BufReader::new(stderr).read_to_string(&mut err);
        }
        (text, err)
    });
    loop {
        if cancel.is_some_and(CancelHandle::is_cancelled) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = output_handle.join();
            if let Some(cancel) = cancel {
                cancel.detach();
            }
            return Err(WinGetError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let (stdout_text, stderr_text) = output_handle
                    .join()
                    .unwrap_or_else(|_| (String::new(), String::new()));
                if let Some(cancel) = cancel {
                    cancel.detach();
                }
                if !status.success() {
                    let detail = stderr_text.trim();
                    let message = if detail.is_empty() {
                        format!(
                            "{} exited with {}",
                            program
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("cli"),
                            status
                        )
                    } else {
                        detail.to_string()
                    };
                    return Err(WinGetError::Operation(message));
                }
                return Ok(stdout_text);
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(error) => {
                let _ = output_handle.join();
                if let Some(cancel) = cancel {
                    cancel.detach();
                }
                return Err(WinGetError::Operation(format!("wait failed: {error}")));
            }
        }
    }
}

/// One-shot capture without cancel (list/search probes).
pub fn run_output(program: &Path, args: &[&str]) -> Result<String, WinGetError> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| {
            WinGetError::Operation(format!("{} could not start: {error}", program.display()))
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        if detail.is_empty() {
            return Err(WinGetError::Operation(format!(
                "{} exited with {}",
                program
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("cli"),
                output.status
            )));
        }
        return Err(WinGetError::Operation(detail.to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

//! Spawn package-manager CLI children off the UI thread with bounded waits.
//!
//! Each child joins a Job Object so cancel and timeout end the whole process tree
//! (`.cmd` shims start `cmd.exe`, which starts the real tool). Stdout and stderr drain
//! on separate threads, so a chatty stream cannot fill its pipe and stall the child.
//! Readers that outlive the child (a grandchild still holds the pipe) are abandoned
//! after a short grace period instead of blocking the worker thread.
use super::broker::CancelHandle;
use super::winget::WinGetError;
use crate::power_store::{OperationProgress, ProgressStage};
use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Upper bound for list, search, details and probe commands. Installs, updates and
/// uninstalls have no limit; the user cancels them.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(300);
const POLL: Duration = Duration::from_millis(100);
/// How long the pipes may stay open after the child exits or is terminated.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Resolve `name` on PATH, returning the first existing file path.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    find_in_dirs(std::env::split_paths(&path), name)
}

/// Only programs Windows can start directly are accepted: `.ps1` scripts and extensionless
/// shell shims (Scoop's `shims\scoop` is a `/bin/sh` script) fail to spawn, so a directory
/// holding only those is treated as "not installed" instead of a broken program.
fn find_in_dirs(dirs: impl IntoIterator<Item = PathBuf>, name: &str) -> Option<PathBuf> {
    const EXTENSIONS: [&str; 3] = [".exe", ".cmd", ".bat"];
    for dir in dirs {
        for extension in EXTENSIONS {
            let candidate = dir.join(format!("{name}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Run a CLI tool to completion, collecting stdout. Cancel ends its process tree.
pub fn run_collect(
    program: &Path,
    args: &[&str],
    cancel: Option<&CancelHandle>,
    on_progress: Option<&mut dyn FnMut(OperationProgress)>,
) -> Result<String, WinGetError> {
    if let Some(on_progress) = on_progress {
        on_progress(OperationProgress {
            stage: ProgressStage::Installing,
            percent: None,
        });
    }
    let mut command = Command::new(program);
    command.args(args);
    run(&program_label(program), command, cancel, None)?.into_stdout(&program_label(program))
}

/// One-shot capture without cancel (list/search probes), bounded by [`QUERY_TIMEOUT`].
pub fn run_output(program: &Path, args: &[&str]) -> Result<String, WinGetError> {
    let mut command = Command::new(program);
    command.args(args);
    let label = program_label(program);
    run(&label, command, None, Some(QUERY_TIMEOUT))?.into_stdout(&label)
}

/// Runs a prepared command bounded by `timeout` and returns its stdout whatever the
/// exit status; the caller parses its own protocol (the WinGet worker child).
pub fn run_command_stdout(
    label: &str,
    command: Command,
    timeout: Duration,
) -> Result<String, WinGetError> {
    run(label, command, None, Some(timeout)).map(|captured| captured.stdout)
}

fn program_label(program: &Path) -> String {
    program
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cli")
        .to_string()
}

struct Captured {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Captured {
    fn into_stdout(self, label: &str) -> Result<String, WinGetError> {
        if self.status.success() {
            return Ok(self.stdout);
        }
        let detail = self.stderr.trim();
        Err(WinGetError::Operation(if detail.is_empty() {
            format!("{label} exited with {}", self.status)
        } else {
            detail.to_string()
        }))
    }
}

fn run(
    label: &str,
    mut command: Command,
    cancel: Option<&CancelHandle>,
    timeout: Option<Duration>,
) -> Result<Captured, WinGetError> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| WinGetError::Operation(format!("{label} could not start: {error}")))?;
    // A grandchild started before this assignment escapes the job; DRAIN_GRACE still
    // bounds the wait for any pipe it keeps open.
    let tree = ProcessTree::adopt(&child);
    if let Some(cancel) = cancel {
        cancel.attach_pid(child.id());
    }
    let pipes = Pipes::drain(&mut child);
    let started = Instant::now();
    let outcome = loop {
        if cancel.is_some_and(CancelHandle::is_cancelled) {
            break Err(WinGetError::Cancelled);
        }
        if let Some(limit) = timeout.filter(|limit| started.elapsed() >= *limit) {
            break Err(WinGetError::Operation(format!(
                "{label} did not finish within {} s",
                limit.as_secs()
            )));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(POLL),
            Err(error) => break Err(WinGetError::Operation(format!("wait failed: {error}"))),
        }
    };
    if let Some(cancel) = cancel {
        cancel.detach();
    }
    if outcome.is_err() {
        if let Some(tree) = &tree {
            tree.terminate();
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    let (stdout, stderr) = pipes.collect(DRAIN_GRACE);
    outcome.map(|status| Captured {
        status,
        stdout,
        stderr,
    })
}

/// Owns a Job Object holding one CLI child and every process it starts afterwards.
/// The job is not kill-on-close: a successful install may leave processes that the
/// package started on purpose. Only cancel and timeout terminate it.
struct ProcessTree(HANDLE);

impl ProcessTree {
    /// `None` when Windows refuses the job; the caller then kills the direct child only.
    fn adopt(child: &Child) -> Option<Self> {
        // SAFETY: null attributes and name create an unnamed job; the returned handle
        // is owned by `Self` and closed in Drop.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return None;
        }
        let tree = Self(job);
        // SAFETY: `job` is live, and the child's process handle stays open while
        // `child` is borrowed; assignment does not transfer either handle.
        let assigned = unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) };
        (assigned != 0).then_some(tree)
    }

    fn terminate(&self) {
        // SAFETY: self.0 is the job handle owned by this value.
        unsafe {
            TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        // SAFETY: self.0 is the owned job handle; closing it leaves the processes running.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[derive(Clone, Copy)]
enum Stream {
    Out,
    Err,
}

/// Chunks from both pipes; a stream's sender drops when its pipe closes.
struct Pipes(Receiver<(Stream, Vec<u8>)>);

impl Pipes {
    fn drain(child: &mut Child) -> Self {
        let (sender, receiver) = mpsc::channel();
        if let Some(stdout) = child.stdout.take() {
            spawn_reader(stdout, Stream::Out, sender.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(stderr, Stream::Err, sender);
        }
        Self(receiver)
    }

    /// Everything both pipes delivered until they closed or `grace` ran out.
    fn collect(self, grace: Duration) -> (String, String) {
        let deadline = Instant::now() + grace;
        let (mut out, mut err) = (Vec::new(), Vec::new());
        while let Ok((stream, chunk)) = self
            .0
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            match stream {
                Stream::Out => out.extend_from_slice(&chunk),
                Stream::Err => err.extend_from_slice(&chunk),
            }
        }
        (
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        )
    }
}

fn spawn_reader(
    mut pipe: impl Read + Send + 'static,
    stream: Stream,
    sender: Sender<(Stream, Vec<u8>)>,
) {
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        while let Ok(read) = pipe.read(&mut buffer) {
            if read == 0 || sender.send((stream, buffer[..read].to_vec())).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(script: &str) -> Command {
        let mut command = Command::new("cmd.exe");
        command.args(["/d", "/c"]).raw_arg(script);
        command
    }

    #[test]
    fn skips_extensionless_shims_and_powershell_scripts() {
        let root = std::env::temp_dir().join(format!("powerkit-path-{}", std::process::id()));
        let shims = root.join("shims");
        let tools = root.join("tools");
        std::fs::create_dir_all(&shims).unwrap();
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(shims.join("scoop"), "#!/bin/sh\n").unwrap();
        std::fs::write(shims.join("scoop.ps1"), "").unwrap();
        std::fs::write(tools.join("scoop.cmd"), "").unwrap();
        std::fs::write(tools.join("npm.ps1"), "").unwrap();

        let found = find_in_dirs([shims.clone(), tools.clone()], "scoop");
        assert_eq!(found, Some(tools.join("scoop.cmd")));
        assert_eq!(find_in_dirs([shims.clone(), tools.clone()], "npm"), None);
        assert_eq!(find_in_dirs([shims.clone()], "scoop"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_full_stderr_pipe_does_not_stall_stdout() {
        // ~200 KB on stderr is far past the pipe buffer; reading stdout first used to hang.
        let script = "\"(for /l %i in (1,1,4000) do @echo 0123456789012345678901234567890123456789 1>&2) & echo done\"";
        let captured = run("cmd", cmd(script), None, Some(Duration::from_secs(60))).unwrap();
        assert!(captured.status.success());
        assert_eq!(captured.stdout.trim(), "done");
        assert!(captured.stderr.len() > 150_000);
    }

    #[test]
    fn timeout_ends_the_process_tree_promptly() {
        // cmd.exe starts ping, which inherits the pipes; killing cmd.exe alone would
        // leave the readers waiting for ping's 30 seconds.
        let started = Instant::now();
        let result = run(
            "cmd",
            cmd("ping -n 30 127.0.0.1"),
            None,
            Some(Duration::from_millis(500)),
        );
        assert!(
            matches!(result, Err(WinGetError::Operation(message)) if message.contains("did not finish"))
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_cancelled_handle_stops_the_child() {
        let cancel = CancelHandle::default();
        cancel.cancel();
        let started = Instant::now();
        let result = run("cmd", cmd("ping -n 30 127.0.0.1"), Some(&cancel), None);
        assert!(matches!(result, Err(WinGetError::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn failure_reports_stderr_or_exit_status() {
        let quiet = run("cmd", cmd("exit 3"), None, Some(Duration::from_secs(30))).unwrap();
        assert!(
            matches!(quiet.into_stdout("tool"), Err(WinGetError::Operation(message)) if message.starts_with("tool exited with"))
        );
        let loud = run(
            "cmd",
            cmd("echo broken 1>&2 & exit 1"),
            None,
            Some(Duration::from_secs(30)),
        )
        .unwrap();
        assert!(
            matches!(loud.into_stdout("tool"), Err(WinGetError::Operation(message)) if message == "broken")
        );
    }
}

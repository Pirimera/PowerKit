//! Runs WinGet COM calls in a child `powerkit.exe --power-store-worker` process.
//!
//! The WinUI shell adds the Windows App Runtime framework to this process's package
//! graph (dynamic dependency). With that graph present, every WinGet COM call fails
//! with "Package Microsoft.WindowsAppRuntime.2_… not found", so the UI process never
//! talks to WinGet directly. The child never bootstraps WinUI and has no graph.
use super::winget::{ManagerError, WinGetSource};
use crate::power_store::{
    InstallOptions, ManagerKind, OperationKind, OperationProgress, PackageDetails, PackageKey,
    PackageSummary, ProgressStage, QueuedOperation, escape_field, unescape_field,
};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

pub const WORKER_FLAG: &str = "--power-store-worker";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const VERB_INSTALLED: &str = "installed";
const VERB_SEARCH_ID: &str = "search-id";
const VERB_SEARCH: &str = "search";
const VERB_DETAILS: &str = "details";

const LINE_PACKAGE: &str = "P";
const LINE_DETAILS: &str = "D";
const LINE_PROGRESS: &str = "G";
const LINE_OK: &str = "OK";
const LINE_ERROR: &str = "ERR";
const KIND_UNAVAILABLE: &str = "unavailable";
const KIND_OPERATION: &str = "operation";
const KIND_CANCELLED: &str = "cancelled";

pub fn list_installed() -> Result<Vec<PackageSummary>, ManagerError> {
    request(&[VERB_INSTALLED]).map(|response| response.packages)
}

#[allow(dead_code)]
pub fn search_equals_id(package_id: &str) -> Result<Vec<PackageSummary>, ManagerError> {
    request(&[VERB_SEARCH_ID, package_id]).map(|response| response.packages)
}

pub fn search_discover(
    query: &str,
    source: Option<&str>,
) -> Result<Vec<PackageSummary>, ManagerError> {
    let source = source.unwrap_or("all");
    request(&[VERB_SEARCH, query, source]).map(|response| response.packages)
}

pub fn details(key: &PackageKey) -> Result<PackageDetails, ManagerError> {
    request(&[VERB_DETAILS, &key.id, &key.source])?
        .details
        .ok_or_else(|| ManagerError::Operation("PowerStore worker returned no details".into()))
}

/// Cancels the running worker operation by writing a line to the child's stdin.
#[derive(Clone, Default)]
pub struct CancelHandle(Arc<Mutex<CancelState>>);

#[derive(Default)]
struct CancelState {
    requested: bool,
    stdin: Option<ChildStdin>,
    /// CLI child pid; terminated when cancel is requested.
    pid: Option<u32>,
}

impl CancelHandle {
    pub fn cancel(&self) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        state.requested = true;
        if let Some(stdin) = state.stdin.as_mut() {
            let _ = writeln!(stdin, "cancel").and_then(|()| stdin.flush());
        }
        if let Some(pid) = state.pid.take() {
            let _ = terminate_pid(pid);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .requested
    }

    pub fn attach_pid(&self, pid: u32) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.requested {
            let _ = terminate_pid(pid);
        } else {
            state.pid = Some(pid);
        }
    }

    fn attach(&self, mut stdin: ChildStdin) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.requested {
            let _ = writeln!(stdin, "cancel").and_then(|()| stdin.flush());
        }
        state.stdin = Some(stdin);
    }

    pub fn detach(&self) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        state.stdin = None;
        state.pid = None;
    }
}

fn terminate_pid(pid: u32) -> Result<(), ()> {
    // SAFETY: process id belongs to a child we spawned for PowerStore.
    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return Err(());
        }
        let ok = TerminateProcess(handle, 1);
        CloseHandle(handle);
        if ok == 0 { Err(()) } else { Ok(()) }
    }
}

/// Worker arguments for an operation: verb, id, source, then the stored options
/// (scope, architecture, version, extra installer arguments; empty when unset).
fn operation_args(operation: &QueuedOperation) -> [&str; 7] {
    fn option(value: &Option<String>) -> &str {
        value.as_deref().unwrap_or_default()
    }
    [
        operation.kind.as_str(),
        &operation.key.id,
        &operation.key.source,
        option(&operation.options.scope),
        option(&operation.options.architecture),
        option(&operation.options.version),
        option(&operation.options.extra_args),
    ]
}

fn operation_from_args(kind: OperationKind, args: &[&str]) -> QueuedOperation {
    let arg = |index: usize| args.get(index).copied().unwrap_or_default();
    let option = |index: usize| Some(arg(index).to_string()).filter(|value| !value.is_empty());
    QueuedOperation {
        kind,
        key: PackageKey::new(ManagerKind::WinGet, arg(1), arg(0)),
        name: arg(0).to_string(),
        options: InstallOptions {
            scope: option(2),
            architecture: option(3),
            version: option(4),
            extra_args: option(5),
        },
    }
}

/// Blocks the calling worker thread until the child exits; never call on the UI thread.
pub fn run_operation(
    operation: &QueuedOperation,
    cancel: &CancelHandle,
    mut on_progress: impl FnMut(OperationProgress),
) -> Result<(), ManagerError> {
    let mut child = worker_command(&operation_args(operation))?
        .stdin(Stdio::piped())
        .spawn()
        .map_err(spawn_error)?;
    if let Some(stdin) = child.stdin.take() {
        cancel.attach(stdin);
    }
    let mut response = String::new();
    if let Some(stdout) = child.stdout.take() {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        while reader
            .read_until(b'\n', &mut line)
            .is_ok_and(|read| read > 0)
        {
            let text = String::from_utf8_lossy(&line);
            match parse_progress(text.trim_end_matches(['\r', '\n'])) {
                Some(progress) => on_progress(progress),
                None => response.push_str(&text),
            }
            line.clear();
        }
    }
    cancel.detach();
    let _ = child.wait();
    parse_response(&response).map(|_| ())
}

fn worker_command(args: &[&str]) -> Result<Command, ManagerError> {
    let exe = std::env::current_exe().map_err(|error| {
        ManagerError::Operation(format!("current executable path unavailable: {error}"))
    })?;
    let mut command = Command::new(exe);
    command
        .arg(WORKER_FLAG)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    Ok(command)
}

fn spawn_error(error: std::io::Error) -> ManagerError {
    ManagerError::Operation(format!("PowerStore worker could not start: {error}"))
}

/// Blocks the calling worker thread until the child exits or the query limit passes,
/// which ends the child and every process it started; never call on the UI thread.
fn request(args: &[&str]) -> Result<Response, ManagerError> {
    let stdout = super::cli_process::run_command_stdout(
        "PowerStore worker",
        worker_command(args)?,
        super::cli_process::QUERY_TIMEOUT,
    )?;
    parse_response(&stdout)
}

/// Entry point for the child process. Returns the exit code when `args` request the worker.
pub fn run_worker(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some(WORKER_FLAG) {
        return None;
    }
    let verb = args.get(2).map(String::as_str).unwrap_or_default();
    let rest: Vec<&str> = args.iter().skip(3).map(String::as_str).collect();
    let response = match execute(verb, &rest) {
        Ok(response) => format_success(&response),
        Err(error) => format_error(&error),
    };
    Some(if write_stdout(&response) { 0 } else { 1 })
}

fn write_stdout(text: &str) -> bool {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .is_ok()
}

#[derive(Debug, Default, PartialEq)]
struct Response {
    packages: Vec<PackageSummary>,
    details: Option<PackageDetails>,
}

fn execute(verb: &str, args: &[&str]) -> Result<Response, ManagerError> {
    let source = WinGetSource::connect()?;
    let arg = |index: usize| args.get(index).copied().unwrap_or_default();
    let packages = |packages| {
        Ok(Response {
            packages,
            details: None,
        })
    };
    match verb {
        VERB_INSTALLED => return packages(source.list_installed()?),
        VERB_SEARCH_ID => return packages(source.search_equals_id(arg(0))?),
        VERB_SEARCH => {
            let catalog = Some(arg(1)).filter(|s| !s.is_empty());
            return packages(source.search_discover(arg(0), catalog)?);
        }
        VERB_DETAILS => {
            return Ok(Response {
                packages: Vec::new(),
                details: Some(source.details(arg(0), arg(1))?),
            });
        }
        _ => {}
    }
    let kind = OperationKind::parse(verb).ok_or_else(|| {
        ManagerError::Operation(format!("unknown PowerStore worker request: {verb}"))
    })?;
    source.run_operation(
        &operation_from_args(kind, args),
        &watch_cancel(),
        report_progress,
    )?;
    Ok(Response::default())
}

/// Any stdin line, or the pipe closing because the parent exited, requests cancellation.
fn watch_cancel() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let watcher = Arc::clone(&flag);
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_line(&mut String::new());
        watcher.store(true, Ordering::Release);
    });
    flag
}

fn report_progress(progress: OperationProgress) {
    write_stdout(&format_progress(progress));
}

fn format_progress(progress: OperationProgress) -> String {
    let percent = progress.percent.map(|p| p.to_string()).unwrap_or_default();
    format!("{LINE_PROGRESS}\t{}\t{percent}\n", progress.stage.as_str())
}

fn parse_progress(line: &str) -> Option<OperationProgress> {
    let mut fields = line.split('\t');
    if fields.next()? != LINE_PROGRESS {
        return None;
    }
    let stage = ProgressStage::parse(fields.next()?)?;
    let percent = match fields.next()? {
        "" => None,
        value => Some(value.parse::<u8>().ok()?.min(100)),
    };
    fields
        .next()
        .is_none()
        .then_some(OperationProgress { stage, percent })
}

fn push_line(text: &mut String, tag: &str, fields: &[&str]) {
    text.push_str(tag);
    for field in fields {
        text.push('\t');
        text.push_str(&escape_field(field));
    }
    text.push('\n');
}

fn format_success(response: &Response) -> String {
    let mut text = String::new();
    for package in &response.packages {
        push_line(
            &mut text,
            LINE_PACKAGE,
            &[
                package.key.manager.as_str(),
                package.key.source.as_str(),
                package.key.id.as_str(),
                package.name.as_str(),
                package.version.as_deref().unwrap_or_default(),
                package.available_version.as_deref().unwrap_or_default(),
                if package.update_available { "1" } else { "0" },
                package.scope.as_deref().unwrap_or_default(),
                package.install_date.as_deref().unwrap_or_default(),
                package.updated_at.as_deref().unwrap_or_default(),
                package.icon.as_deref().unwrap_or_default(),
            ],
        );
    }
    if let Some(details) = &response.details {
        push_line(&mut text, LINE_DETAILS, &details.fields());
    }
    text.push_str(LINE_OK);
    text.push('\n');
    text
}

fn format_error(error: &ManagerError) -> String {
    let kind = match error {
        ManagerError::Unavailable(_) => KIND_UNAVAILABLE,
        ManagerError::Operation(_) => KIND_OPERATION,
        ManagerError::Cancelled => KIND_CANCELLED,
    };
    format!(
        "{LINE_ERROR}\t{kind}\t{}\n",
        escape_field(&error.to_string())
    )
}

fn parse_response(text: &str) -> Result<Response, ManagerError> {
    let mut response = Response::default();
    for line in text.lines() {
        let fields: Vec<String> = line.split('\t').map(unescape_field).collect();
        match fields.first().map(String::as_str) {
            Some(LINE_PACKAGE) if fields.len() >= 8 => {
                let optional = |index: usize| {
                    fields
                        .get(index)
                        .map(String::as_str)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                };
                let manager = ManagerKind::parse(&fields[1]).unwrap_or(ManagerKind::WinGet);
                response.packages.push(PackageSummary {
                    key: PackageKey::new(manager, &fields[2], &fields[3]),
                    name: fields[4].clone(),
                    version: optional(5),
                    available_version: optional(6),
                    update_available: fields[7] == "1",
                    scope: optional(8),
                    install_date: optional(9),
                    updated_at: optional(10),
                    icon: optional(11),
                });
            }
            Some(LINE_DETAILS) => response.details = PackageDetails::from_fields(&fields[1..]),
            Some(LINE_OK) => return Ok(response),
            Some(LINE_ERROR) if fields.len() == 3 => {
                let message = fields[2].clone();
                return Err(match fields[1].as_str() {
                    KIND_UNAVAILABLE => ManagerError::Unavailable(message),
                    KIND_CANCELLED => ManagerError::Cancelled,
                    _ => ManagerError::Operation(message),
                });
            }
            _ => {}
        }
    }
    Err(ManagerError::Operation(
        "PowerStore worker exited without a result".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(id: &str, name: &str, version: Option<&str>, update: bool) -> PackageSummary {
        PackageSummary {
            key: PackageKey::new(ManagerKind::WinGet, "winget", id),
            name: name.into(),
            version: version.map(str::to_string),
            available_version: update.then(|| "9.9".to_string()),
            update_available: update,
            scope: update.then(|| "user".to_string()),
            install_date: update.then(|| "2026-10-01".to_string()),
            updated_at: update.then(|| "2026-11-16".to_string()),
            icon: update.then(|| "C:\\Program Files\\Contoso\\app.exe,0".to_string()),
        }
    }

    #[test]
    fn responses_round_trip_with_control_characters() {
        let response = Response {
            packages: vec![
                package("Contoso.App", "Tab\tName\\with\nbreaks", Some("1.2"), true),
                package("Contoso.Empty", "", None, false),
            ],
            details: Some(PackageDetails {
                id: "Contoso.App".into(),
                description: "Line one\nLine\ttwo".into(),
                ..PackageDetails::default()
            }),
        };
        let parsed = parse_response(&format_success(&response)).expect("parse");
        assert_eq!(parsed, response);
    }

    #[test]
    fn operation_options_survive_the_command_line() {
        let operation = QueuedOperation {
            kind: OperationKind::Update,
            key: PackageKey::new(ManagerKind::WinGet, "msstore", "9NBLGGH4NNS1"),
            name: "App".into(),
            options: InstallOptions {
                scope: Some("user".into()),
                architecture: None,
                version: Some("1.2".into()),
                extra_args: Some("--a \"b c\"".into()),
            },
        };
        let args = operation_args(&operation);
        assert_eq!(OperationKind::parse(args[0]), Some(OperationKind::Update));
        let parsed = operation_from_args(OperationKind::Update, &args[1..]);
        assert_eq!(parsed.key, operation.key);
        assert_eq!(parsed.options, operation.options);
    }

    #[test]
    fn errors_keep_their_kind_and_missing_result_is_an_error() {
        let unavailable = format_error(&ManagerError::Unavailable("no\tCOM".into()));
        match parse_response(&unavailable) {
            Err(ManagerError::Unavailable(message)) => assert_eq!(message, "no\tCOM"),
            other => panic!("expected unavailable, got {other:?}"),
        }
        assert!(matches!(
            parse_response("P\tpartial"),
            Err(ManagerError::Operation(_))
        ));
        assert!(matches!(
            parse_response(&format_error(&ManagerError::Cancelled)),
            Err(ManagerError::Cancelled)
        ));
    }

    #[test]
    fn progress_lines_round_trip_and_reject_malformed_input() {
        for progress in [
            OperationProgress {
                stage: ProgressStage::Downloading,
                percent: Some(42),
            },
            OperationProgress {
                stage: ProgressStage::Installing,
                percent: None,
            },
        ] {
            let line = format_progress(progress);
            assert_eq!(parse_progress(line.trim_end_matches('\n')), Some(progress));
        }
        for malformed in [
            "G\tdownloading",
            "G\tbogus\t5",
            "G\tqueued\tx",
            "G\tqueued\t5\textra",
        ] {
            assert_eq!(parse_progress(malformed), None, "{malformed}");
        }
        assert_eq!(parse_progress("OK"), None);
    }

    #[test]
    fn worker_flag_is_only_recognized_as_the_first_argument() {
        let other = vec!["powerkit.exe".to_string(), "--smoke-test".to_string()];
        assert_eq!(run_worker(&other), None);
    }
}

//! The fan-out and the adapters over a fake programs port: no program is
//! started, so these run the same on every platform.
use super::*;
use std::sync::Mutex;

/// Programs that exist only as names, with canned output per first argument.
#[derive(Default)]
struct FakePrograms {
    installed: Vec<&'static str>,
    /// (program, first argument) → standard output.
    output: Vec<((&'static str, &'static str), &'static str)>,
    runs: Mutex<Vec<(String, Vec<String>, bool)>>,
}
impl FakePrograms {
    fn ran(&self) -> Vec<(String, Vec<String>, bool)> {
        self.runs.lock().unwrap().clone()
    }
}
impl Programs for Arc<FakePrograms> {
    fn find(&self, name: &str) -> Option<PathBuf> {
        self.installed.contains(&name).then(|| PathBuf::from(name))
    }
    /// No manager has a home here: nothing on this machine is consulted.
    fn var(&self, _: &str) -> Option<std::ffi::OsString> {
        None
    }
    fn run(
        &self,
        program: &Path,
        args: &[&str],
        cancel: Option<&Cancel>,
    ) -> Result<String, CatalogError> {
        let name = program.display().to_string();
        self.runs.lock().unwrap().push((
            name.clone(),
            args.iter().map(|arg| arg.to_string()).collect(),
            cancel.is_some(),
        ));
        if cancel.is_some_and(Cancel::is_cancelled) {
            return Err(CatalogError::Cancelled);
        }
        let first = args.first().copied().unwrap_or_default();
        self.output
            .iter()
            .find(|((program, arg), _)| *program == name && *arg == first)
            .map(|(_, text)| text.to_string())
            .ok_or_else(|| CatalogError::Failed(format!("{name} {first} failed")))
    }
}

/// A native service that answers with fixed rows or one error.
#[derive(Default)]
struct FakeNative {
    installed: bool,
    rows: Vec<PackageSummary>,
    unavailable: Option<&'static str>,
    calls: Mutex<Vec<String>>,
}
impl NativePackages for Arc<FakeNative> {
    fn kind(&self) -> ManagerKind {
        ManagerKind::WinGet
    }
    fn sources(&self) -> &'static [&'static str] {
        &["winget", "msstore"]
    }
    fn seems_installed(&self) -> bool {
        self.installed
    }
    fn list_installed(&self) -> Result<Vec<PackageSummary>, CatalogError> {
        self.calls.lock().unwrap().push("list".into());
        match self.unavailable {
            Some(message) => Err(CatalogError::Unavailable(message.into())),
            None => Ok(self.rows.clone()),
        }
    }
    fn search(
        &self,
        query: &str,
        source: Option<&str>,
    ) -> Result<Vec<PackageSummary>, CatalogError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("search {query} {source:?}"));
        Ok(self.rows.clone())
    }
    fn details(&self, key: &PackageKey) -> Result<PackageDetails, CatalogError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("details {}", key.id));
        Err(CatalogError::Failed("no details".into()))
    }
    fn presence_when_unavailable(&self, message: &str) -> ManagerPresence {
        if message.contains("old") {
            ManagerPresence::TooOld
        } else {
            ManagerPresence::Missing
        }
    }
    fn after_listing(&self, packages: &[PackageSummary]) -> BTreeSet<PackageKey> {
        // The first row plays the part of a store-delivered package.
        packages
            .first()
            .map(|row| row.key.clone())
            .into_iter()
            .collect()
    }
}

fn row(manager: ManagerKind, source: &str, id: &str) -> PackageSummary {
    PackageSummary {
        key: PackageKey::new(manager, source, id),
        name: id.into(),
        version: Some("1.0".into()),
        available_version: None,
        update_available: false,
        scope: None,
        install_date: None,
        updated_at: None,
        icon: None,
    }
}
fn npm() -> FakePrograms {
    FakePrograms {
        installed: vec!["npm"],
        output: vec![
            (
                ("npm", "list"),
                r#"{"dependencies":{"typescript":{"version":"5.4.0"}}}"#,
            ),
            (
                ("npm", "outdated"),
                r#"{"typescript":{"current":"5.4.0","wanted":"5.4.5","latest":"5.5.0"}}"#,
            ),
            (("npm", "install"), ""),
        ],
        ..FakePrograms::default()
    }
}
fn managers(
    programs: FakePrograms,
    native: FakeNative,
) -> (Managers, Arc<FakePrograms>, Arc<FakeNative>) {
    let (programs, native) = (Arc::new(programs), Arc::new(native));
    let managers = Managers::new(Box::new(programs.clone()), Box::new(native.clone()));
    managers.configure(&PowerStorePreferences::default());
    (managers, programs, native)
}
fn presence(list: &[ManagerStatus], kind: ManagerKind) -> ManagerPresence {
    list.iter()
        .find(|status| status.kind == kind)
        .unwrap()
        .presence
}

#[test]
fn answers_keep_input_order_and_a_panic_stays_with_its_manager() {
    let jobs: Vec<(ManagerKind, Job<'static>)> = vec![
        (ManagerKind::Npm, job(|| Ok(Some(Vec::new())))),
        (ManagerKind::Pip, job(|| panic!("synthetic adapter panic"))),
        (ManagerKind::Cargo, job(|| Ok(None))),
    ];
    let answers = gather(jobs);
    let kinds: Vec<ManagerKind> = answers.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(
        kinds,
        [ManagerKind::Npm, ManagerKind::Pip, ManagerKind::Cargo]
    );
    assert!(matches!(&answers[0].1, Ok(Some(rows)) if rows.is_empty()));
    assert!(answers[1].1.is_err());
    assert!(matches!(answers[2].1, Ok(None)));
}

#[test]
fn a_cancel_is_remembered_and_ends_the_program_attached_later() {
    let stopped = Arc::new(AtomicBool::new(false));
    let cancel = Cancel::default();
    assert!(!cancel.is_cancelled());
    cancel.cancel();
    assert!(cancel.is_cancelled());
    // Attaching after the request stops the program at once.
    let flag = stopped.clone();
    cancel.attach(move || flag.store(true, Ordering::SeqCst));
    assert!(stopped.load(Ordering::SeqCst));

    // The usual order: attach, then cancel; nothing is stopped after detach.
    let count = Arc::new(Mutex::new(0));
    let cancel = Cancel::default();
    let seen = count.clone();
    cancel.attach(move || *seen.lock().unwrap() += 1);
    assert_eq!(*count.lock().unwrap(), 0);
    cancel.clone().cancel();
    assert_eq!(*count.lock().unwrap(), 1);
    cancel.detach();
    cancel.cancel();
    assert_eq!(*count.lock().unwrap(), 1);
}

#[test]
fn a_program_is_resolved_from_the_switch_the_custom_path_and_discovery() {
    let discovered = || Some(PathBuf::from("on-path"));
    let off = ManagerSettings {
        enabled: false,
        ..ManagerSettings::default()
    };
    assert_eq!(resolve(&off, discovered), None);
    assert_eq!(
        resolve(&ManagerSettings::default(), discovered),
        discovered()
    );
    // A missing custom program is not replaced by another install.
    let missing = ManagerSettings {
        executable: Some("no-such-dir/definitely-missing-tool".into()),
        ..ManagerSettings::default()
    };
    assert_eq!(resolve(&missing, discovered), None);
    let exe = std::env::current_exe().unwrap();
    let custom = ManagerSettings {
        executable: Some(exe.to_string_lossy().into_owned()),
        ..ManagerSettings::default()
    };
    assert_eq!(resolve(&custom, discovered), Some(exe));
}

#[test]
fn installed_packages_come_from_every_manager_that_is_present() {
    let native = FakeNative {
        installed: true,
        rows: vec![row(ManagerKind::WinGet, "winget", "Git.Git")],
        ..FakeNative::default()
    };
    let (managers, programs, _) = managers(npm(), native);
    let list = managers.list_installed().unwrap();
    let ids: Vec<&str> = list
        .packages
        .iter()
        .map(|row| row.key.id.as_str())
        .collect();
    // The native service answers first, then the command-line managers in order.
    assert_eq!(ids, ["Git.Git", "typescript"]);
    let typescript = &list.packages[1];
    assert_eq!(typescript.version.as_deref(), Some("5.4.0"));
    assert_eq!(typescript.available_version.as_deref(), Some("5.5.0"));
    assert!(typescript.update_available);
    assert_eq!(
        presence(&list.managers, ManagerKind::WinGet),
        ManagerPresence::Available
    );
    assert_eq!(
        presence(&list.managers, ManagerKind::Npm),
        ManagerPresence::Available
    );
    assert_eq!(
        presence(&list.managers, ManagerKind::Scoop),
        ManagerPresence::Missing
    );
    // What the platform says its own store delivers is passed on.
    assert!(list.store_managed.contains(&list.packages[0].key));
    // Queries are bounded, not cancellable.
    assert!(
        programs
            .ran()
            .iter()
            .all(|(_, _, cancellable)| !cancellable)
    );
}

#[test]
fn a_manager_that_is_switched_off_is_located_but_never_started() {
    let (managers, programs, native) = managers(npm(), FakeNative::default());
    let mut prefs = PowerStorePreferences::default();
    for kind in [ManagerKind::Npm, ManagerKind::WinGet] {
        let settings = ManagerSettings {
            enabled: false,
            ..prefs.manager(kind)
        };
        assert!(prefs.set_manager(kind, settings));
    }
    managers.configure(&prefs);
    assert!(!managers.is_enabled(ManagerKind::Npm));
    // npm is still reported as installed, with its path but no version.
    assert_eq!(
        presence(&managers.statuses(), ManagerKind::Npm),
        ManagerPresence::Available
    );
    let probe = managers.probe(ManagerKind::Npm);
    assert_eq!(probe.path.as_deref(), Some("npm"));
    assert_eq!(probe.version, None);
    // Nothing else is installed, so the listing has no manager to ask.
    assert!(managers.list_installed().is_err());
    assert!(programs.ran().is_empty());
    assert!(native.calls.lock().unwrap().is_empty());
    let key = PackageKey::new(ManagerKind::WinGet, "winget", "Git.Git");
    assert!(matches!(
        managers.details(&key),
        Err(CatalogError::Unavailable(_))
    ));
}

#[test]
fn an_old_native_service_is_reported_as_such_when_nothing_else_answers() {
    let native = FakeNative {
        installed: true,
        unavailable: Some("too old"),
        ..FakeNative::default()
    };
    let (managers, _, _) = managers(FakePrograms::default(), native);
    assert_eq!(
        managers.list_installed().err(),
        Some(CatalogError::Unavailable("too old".into()))
    );
    // With another manager answering, the list succeeds and carries the status.
    let native = FakeNative {
        installed: true,
        unavailable: Some("too old"),
        ..FakeNative::default()
    };
    let (managers, _, _) = self::managers(npm(), native);
    let list = managers.list_installed().unwrap();
    assert_eq!(
        presence(&list.managers, ManagerKind::WinGet),
        ManagerPresence::TooOld
    );
    assert_eq!(list.packages.len(), 1);
}

#[test]
fn a_search_asks_only_the_managers_the_source_names() {
    let programs = FakePrograms {
        output: vec![(("npm", "search"), "[]")],
        ..npm()
    };
    let native = FakeNative {
        rows: vec![row(ManagerKind::WinGet, "winget", "Git.Git")],
        ..FakeNative::default()
    };
    let (managers, programs, native) = managers(programs, native);
    assert!(managers.search("git", Some("npm")).unwrap().is_empty());
    assert_eq!(programs.ran().len(), 1);
    assert!(native.calls.lock().unwrap().is_empty());
    // A native source goes to the native service alone, with its name.
    let found = managers.search("git", Some("msstore")).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(programs.ran().len(), 1);
    assert_eq!(
        native.calls.lock().unwrap().as_slice(),
        [r#"search git Some("msstore")"#]
    );
    // An empty query is answered without starting npm.
    assert!(managers.search("  ", Some("npm")).unwrap().is_empty());
    assert_eq!(programs.ran().len(), 1);
}

#[test]
fn an_operation_runs_cancellably_and_the_native_service_keeps_its_own() {
    let (managers, programs, _) = managers(npm(), FakeNative::default());
    let operation = |manager, source: &str, id: &str| QueuedOperation {
        kind: powerkit_core::power_store::OperationKind::Install,
        key: PackageKey::new(manager, source, id),
        name: id.into(),
        options: Default::default(),
    };
    let mut stages = Vec::new();
    let result = managers.run_operation(
        &operation(ManagerKind::Npm, "npm", "typescript"),
        &Cancel::default(),
        |progress| stages.push(progress.stage),
    );
    assert_eq!(result, Some(Ok(())));
    assert_eq!(
        programs.ran(),
        [(
            "npm".to_string(),
            vec!["install".to_string(), "-g".into(), "typescript".into()],
            true
        )]
    );
    assert_eq!(stages.first(), Some(&ProgressStage::Queued));
    assert_eq!(stages.last(), Some(&ProgressStage::Finishing));
    // A request made before the program starts cancels it.
    let cancel = Cancel::default();
    cancel.cancel();
    assert_eq!(
        managers.run_operation(
            &operation(ManagerKind::Npm, "npm", "typescript"),
            &cancel,
            |_| {}
        ),
        Some(Err(CatalogError::Cancelled))
    );
    // The native service's operations are not run here.
    assert_eq!(
        managers.run_operation(
            &operation(ManagerKind::WinGet, "winget", "Git.Git"),
            &Cancel::default(),
            |_| {}
        ),
        None
    );
}

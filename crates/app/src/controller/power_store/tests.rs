//! Session behaviour: list loading, selection, filters, queue and check scheduling.

use super::*;
use crate::testkit::FakeCatalog;
use powerkit_core::power_store::{ManagerKind, NO_MANAGERS_AVAILABLE};

/// A session over an empty fake catalog.
fn new_session() -> Session {
    Session::new(Arc::new(Arc::new(FakeCatalog::default())))
}

fn summary(id: &str, update: bool) -> PackageSummary {
    PackageSummary {
        key: PackageKey::new(ManagerKind::WinGet, "winget", id),
        name: id.into(),
        version: Some("1.0".into()),
        available_version: Some("2.0".into()),
        update_available: update,
        scope: Some("user".into()),
        install_date: Some("2026-10-01".into()),
        updated_at: Some("2026-11-01".into()),
        icon: None,
    }
}

#[test]
fn silent_refresh_keeps_cached_rows_instead_of_loading() {
    let mut session = Session {
        installed_packages: vec![summary("Alpha.App", false)],
        installed_status: Status::Ready,
        ..new_session()
    };
    session.refresh_installed(true);
    assert!(session.refreshing);
    assert!(matches!(session.installed_status, Status::Ready));
    assert_eq!(session.installed_packages.len(), 1);
    // Drop the in-flight worker so this unit test does not wait on managers.
    session.installed_generation += 1;
    session.installed_receiver = None;
    session.refreshing = false;

    let mut cold = new_session();
    cold.refresh_installed(true);
    assert!(!cold.refreshing);
    assert!(matches!(cold.installed_status, Status::Loading));
    cold.installed_generation += 1;
    cold.installed_receiver = None;
}

#[test]
fn discover_start_keeps_a_running_installed_load() {
    let (sender, receiver) = mpsc::channel();
    let mut session = Session {
        installed_status: Status::Loading,
        installed_generation: 1,
        installed_receiver: Some(receiver),
        discover_query: "git".into(),
        remote_icons: false,
        ..new_session()
    };
    session.refresh_discover();
    assert!(session.installed_receiver.is_some());
    assert!(matches!(session.installed_status, Status::Loading));
    assert!(matches!(session.discover_status, Status::Loading));

    sender
        .send(ListOutcome::Done {
            mode: ListMode::Installed,
            generation: 1,
            packages: vec![summary("Alpha.App", false)],
            managers: None,
            store_managed: BTreeSet::new(),
        })
        .unwrap();
    assert!(session.poll_list());
    assert!(matches!(session.installed_status, Status::Ready));
    assert_eq!(session.installed_packages.len(), 1);

    // Drop the in-flight Discover worker so this unit test does not wait on managers.
    session.discover_generation += 1;
    session.discover_receiver = None;
}

#[test]
fn answers_from_a_superseded_load_are_ignored() {
    let (sender, receiver) = mpsc::channel();
    let mut session = Session {
        installed_status: Status::Loading,
        installed_generation: 2,
        installed_receiver: Some(receiver),
        ..new_session()
    };
    sender
        .send(ListOutcome::Done {
            mode: ListMode::Installed,
            generation: 1,
            packages: vec![summary("Old.App", false)],
            managers: None,
            store_managed: BTreeSet::new(),
        })
        .unwrap();
    assert!(session.poll_list());
    assert!(matches!(session.installed_status, Status::Loading));
    assert!(session.installed_packages.is_empty());
}

#[test]
fn a_stopped_worker_fails_only_its_own_loading_list() {
    let (installed_sender, installed_receiver) = mpsc::channel::<ListOutcome>();
    let (discover_sender, discover_receiver) = mpsc::channel::<ListOutcome>();
    let mut session = Session {
        installed_status: Status::Loading,
        installed_generation: 1,
        installed_receiver: Some(installed_receiver),
        discover_status: Status::Loading,
        discover_generation: 1,
        discover_receiver: Some(discover_receiver),
        ..new_session()
    };
    drop(discover_sender);
    assert!(session.poll_list());
    assert!(
        matches!(&session.discover_status, Status::Failed(message) if message == LIST_WORKER_STOPPED)
    );
    assert!(matches!(session.installed_status, Status::Loading));
    assert!(session.installed_receiver.is_some());

    drop(installed_sender);
    assert!(session.poll_list());
    assert!(
        matches!(&session.installed_status, Status::Failed(message) if message == LIST_WORKER_STOPPED)
    );
}

fn cold_load_with_cache(packages: Vec<PackageSummary>) -> (Session, mpsc::Sender<ListOutcome>) {
    let (sender, receiver) = mpsc::channel();
    let session = Session {
        installed_status: Status::Loading,
        installed_generation: 1,
        installed_receiver: Some(receiver),
        ..new_session()
    };
    sender
        .send(ListOutcome::Cached {
            generation: 1,
            cache: InstalledCache {
                checked_at: "2026-11-19 09:00".into(),
                packages,
                store_managed: BTreeSet::new(),
            },
        })
        .unwrap();
    (session, sender)
}

#[test]
fn a_cold_load_shows_the_saved_list_until_the_managers_answer() {
    let (mut session, sender) = cold_load_with_cache(vec![summary("Saved.App", true)]);
    assert!(session.poll_list());
    assert!(matches!(session.installed_status, Status::Ready));
    assert!(session.refreshing && session.from_cache);
    assert!(session.installed_receiver.is_some());
    assert_eq!(session.last_checked.as_deref(), Some("2026-11-19 09:00"));
    assert_eq!(session.update_count(&UpdateFilters::default()), Some(1));

    sender
        .send(ListOutcome::Done {
            mode: ListMode::Installed,
            generation: 1,
            packages: vec![summary("Fresh.App", false)],
            managers: None,
            store_managed: BTreeSet::new(),
        })
        .unwrap();
    assert!(session.poll_list());
    assert!(!session.refreshing && !session.from_cache);
    assert!(session.installed_receiver.is_none());
    assert_eq!(session.installed_packages[0].key.id, "Fresh.App");
    assert_eq!(session.update_count(&UpdateFilters::default()), Some(0));
}

#[test]
fn a_saved_list_and_the_final_answer_arriving_together_end_on_the_answer() {
    let (mut session, sender) = cold_load_with_cache(vec![summary("Saved.App", false)]);
    sender
        .send(ListOutcome::Done {
            mode: ListMode::Installed,
            generation: 1,
            packages: vec![summary("Fresh.App", false)],
            managers: None,
            store_managed: BTreeSet::new(),
        })
        .unwrap();
    assert!(session.poll_list());
    assert_eq!(session.installed_packages[0].key.id, "Fresh.App");
    assert!(!session.refreshing && !session.from_cache);
}

#[test]
fn a_failed_refresh_keeps_the_saved_rows_and_says_so() {
    let (mut session, sender) = cold_load_with_cache(vec![summary("Saved.App", false)]);
    assert!(session.poll_list());
    sender
        .send(ListOutcome::Failed {
            mode: ListMode::Installed,
            generation: 1,
            error: NO_MANAGERS_AVAILABLE.into(),
            unavailable: true,
            managers: None,
        })
        .unwrap();
    assert!(session.poll_list());
    assert!(matches!(session.installed_status, Status::Ready));
    assert_eq!(session.installed_packages[0].key.id, "Saved.App");
    assert!(!session.refreshing);
    assert_eq!(
        session.notice,
        Some(Notice::RefreshFailed(NO_MANAGERS_AVAILABLE.into()))
    );
}

#[test]
fn a_saved_list_never_replaces_rows_a_check_already_loaded() {
    let (mut session, _sender) = cold_load_with_cache(vec![summary("Saved.App", false)]);
    session.apply_installed(vec![summary("Checked.App", false)], BTreeSet::new());
    assert!(session.poll_list());
    assert_eq!(session.installed_packages[0].key.id, "Checked.App");
    assert!(!session.from_cache);
}

#[test]
fn ensure_loaded_silently_refreshes_a_ready_cache() {
    let mut session = Session {
        installed_packages: vec![summary("Alpha.App", false)],
        installed_status: Status::Ready,
        ..new_session()
    };
    session.ensure_loaded();
    assert!(session.refreshing);
    assert!(matches!(session.installed_status, Status::Ready));
    session.installed_generation += 1;
    session.installed_receiver = None;
    session.refreshing = false;
}

#[test]
fn update_count_applies_current_filters() {
    let mut session = new_session();
    assert_eq!(session.update_count(&UpdateFilters::default()), None);
    session.store_updates(&[summary("A", true), summary("B", true), summary("C", false)]);
    let mut filters = UpdateFilters::default();
    assert_eq!(session.update_count(&filters), Some(2));
    filters.ignore(PackageKey::new(ManagerKind::WinGet, "winget", "A"));
    assert_eq!(session.update_count(&filters), Some(1));
}

#[test]
fn first_periodic_check_waits_for_the_startup_delay() {
    let mut session = new_session();
    let start = Instant::now();
    session.tick(start, 60);
    assert!(!session.checking);
    assert_eq!(session.next_check, Some(start + FIRST_CHECK_DELAY));
}

#[test]
fn selection_accepts_only_listed_rows_and_select_all_respects_filters() {
    let mut session = new_session();
    let prefs = PowerStorePreferences::default();
    let a = summary("Alpha.App", true);
    let b = summary("Beta.App", false);
    session.installed_packages = vec![a.clone(), b.clone()];
    session.set_filter_text("alpha".into());
    session.set_select_all_visible(true, &prefs);
    assert_eq!(session.selected.len(), 1);
    assert!(session.selected.contains(&a.key));
    session.set_select_all_visible(false, &prefs);
    assert!(session.selected.is_empty());
}

#[test]
fn bulk_actions_skip_powerkit_and_non_updatable_rows() {
    let mut session = new_session();
    let own = summary(powerkit_core::power_store::OWN_PACKAGE_ID, true);
    let up = summary("Up.App", true);
    let cur = summary("Cur.App", false);
    session.installed_packages = vec![own.clone(), up.clone(), cur.clone()];
    for package in session.installed_packages.clone() {
        session.set_selected(package.key, true);
    }
    let prefs = PowerStorePreferences::default();
    session.running = Some(QueuedOperation {
        kind: OperationKind::Install,
        key: own.key,
        name: String::new(),
        options: InstallOptions::default(),
    });
    assert_eq!(session.enqueue_selected(OperationKind::Update, &prefs), 1);
    assert_eq!(session.queue.iter().next().unwrap().key, up.key);
}

#[test]
fn rows_report_their_own_running_and_waiting_operation() {
    let mut session = new_session();
    let prefs = PowerStorePreferences::default();
    let running = summary("Running.App", true);
    let waiting = summary("Waiting.App", true);
    let idle = summary("Idle.App", true);
    session.installed_packages = vec![running.clone(), waiting.clone(), idle.clone()];
    session.running = Some(QueuedOperation {
        kind: OperationKind::Update,
        key: running.key.clone(),
        name: running.name.clone(),
        options: InstallOptions::default(),
    });
    session.progress = Some(OperationProgress::new(
        powerkit_core::power_store::ProgressStage::Downloading,
        0.45,
    ));
    session.queue.push(QueuedOperation {
        kind: OperationKind::Update,
        key: waiting.key.clone(),
        name: waiting.name.clone(),
        options: InstallOptions::default(),
    });

    let snapshot = session.snapshot(&prefs);
    let operation = |id: &str| {
        snapshot
            .packages
            .iter()
            .find(|row| row.key.id == id)
            .and_then(|row| row.operation)
    };
    assert!(matches!(
        operation("Running.App"),
        Some(RowOperation::Running {
            kind: OperationKind::Update,
            cancelling: false,
            progress: Some(OperationProgress {
                percent: Some(45),
                ..
            }),
        })
    ));
    assert_eq!(
        operation("Waiting.App"),
        Some(RowOperation::Pending {
            kind: OperationKind::Update,
            index: 0,
        })
    );
    assert_eq!(operation("Idle.App"), None);
}

#[test]
fn store_delivered_rows_are_never_bulk_uninstalled() {
    let mut session = new_session();
    let store = summary("Store.Delivered", false);
    session.installed_packages = vec![store.clone()];
    session.store_managed.insert(store.key.clone());
    session.set_selected(store.key.clone(), true);
    let prefs = PowerStorePreferences::default();
    assert_eq!(
        session.enqueue_selected(OperationKind::Uninstall, &prefs),
        0
    );
    assert!(session.queue.is_empty());
}

#[test]
fn update_all_on_page_respects_list_filters() {
    let mut session = new_session();
    let prefs = PowerStorePreferences::default();
    let winget = summary("Winget.Up", true);
    let mut local = summary("Local.Up", true);
    local.key = PackageKey::new(ManagerKind::WinGet, "local", "Local.Up");
    let local_current = {
        let mut package = summary("Local.Cur", false);
        package.key = PackageKey::new(ManagerKind::WinGet, "local", "Local.Cur");
        package
    };
    session.installed_packages = vec![winget.clone(), local.clone(), local_current];
    session.updates = Some(vec![winget.clone(), local.clone()]);
    session.set_source_checked("local", true);

    let snap = session.snapshot(&prefs);
    assert_eq!(snap.visible_count, 2);
    assert_eq!(snap.visible_updatable_count, 1);
    assert_eq!(snap.check.count, Some(2));

    // Keep an in-flight op so kick_queue leaves queued items in place.
    session.running = Some(QueuedOperation {
        kind: OperationKind::Install,
        key: winget.key.clone(),
        name: String::new(),
        options: InstallOptions::default(),
    });
    assert_eq!(session.enqueue_visible_updates(&prefs), 1);
    assert_eq!(session.queue.len(), 1);
    assert_eq!(session.queue.iter().next().unwrap().key, local.key);

    session.queue.clear();
    session.enqueue_updates(&prefs);
    assert_eq!(session.queue.len(), 2);
}

// ---- The session over a scripted catalog, end to end -----------------------

const SETTLE: Duration = Duration::from_secs(20);

struct Bench {
    catalog: Arc<FakeCatalog>,
    session: Session,
    prefs: PowerStorePreferences,
}
impl Bench {
    fn new(installed: Vec<PackageSummary>) -> Self {
        let catalog = Arc::new(FakeCatalog::default());
        catalog.set_installed(installed);
        Self {
            session: Session::new(Arc::new(catalog.clone())),
            catalog,
            prefs: PowerStorePreferences::default(),
        }
    }
    fn view(&self) -> Snapshot {
        self.session.snapshot(&self.prefs)
    }
    /// Polls like the main thread's timer until `done` holds.
    fn until(&mut self, what: &str, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
        let start = Instant::now();
        loop {
            self.session.poll(&self.prefs.filters);
            let view = self.view();
            if done(&view) {
                return view;
            }
            assert!(start.elapsed() < SETTLE, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn loaded(installed: Vec<PackageSummary>) -> Self {
        let mut bench = Self::new(installed);
        bench.session.ensure_loaded();
        bench.until("the first list", |view| view.status == Status::Ready);
        bench
    }
    fn idle(view: &Snapshot) -> bool {
        view.op.running.is_none() && view.op.pending == 0 && !view.check.checking
    }
}
fn queued(kind: OperationKind, package: &PackageSummary) -> QueuedOperation {
    QueuedOperation {
        kind,
        key: package.key.clone(),
        name: package.name.clone(),
        options: InstallOptions::default(),
    }
}

#[test]
fn a_cold_load_lists_installed_packages_and_counts_their_updates() {
    let mut bench = Bench::new(vec![summary("Alpha.App", true), summary("Beta.App", false)]);
    assert_eq!(bench.view().status, Status::Idle);
    bench.session.ensure_loaded();
    assert_eq!(bench.view().status, Status::Loading);
    let view = bench.until("the list", |view| view.status == Status::Ready);
    assert_eq!(view.total_installed, 2);
    assert_eq!(view.visible_updatable_count, 1);
    assert_eq!(view.check.count, Some(1));
    assert_eq!(view.check.last_checked.as_deref(), Some("2026-11-23 10:00"));
    assert!(!view.refreshing && !view.from_cache);
    assert!(!view.managers.is_empty());
    // Coming back to the page refreshes without blanking the rows.
    let before = bench.catalog.list_calls();
    bench.session.ensure_loaded();
    assert_eq!(bench.view().status, Status::Ready);
    assert!(bench.view().refreshing);
    bench.until("the silent refresh", |view| !view.refreshing);
    assert!(bench.catalog.list_calls() > before);
}

#[test]
fn queued_operations_run_one_at_a_time_in_order_and_are_recorded() {
    let (a, b) = (summary("Alpha.App", true), summary("Beta.App", true));
    let mut bench = Bench::loaded(vec![a.clone(), b.clone()]);
    let history_path = PathBuf::from("fake-store-history.txt");
    bench.session.load_history(history_path.clone());
    bench.catalog.hold_operations();
    bench.session.enqueue(queued(OperationKind::Update, &a));
    bench.session.enqueue(queued(OperationKind::Update, &b));
    let view = bench.view();
    assert_eq!(
        view.op.running.as_ref().map(|op| op.name.as_str()),
        Some("Alpha.App")
    );
    assert_eq!(view.op.pending, 1);
    let row = |view: &Snapshot, id: &str| {
        view.packages
            .iter()
            .find(|row| row.key.id == id)
            .and_then(|row| row.operation)
    };
    assert!(matches!(
        row(&view, "Alpha.App"),
        Some(RowOperation::Running { .. })
    ));
    assert_eq!(
        row(&view, "Beta.App"),
        Some(RowOperation::Pending {
            kind: OperationKind::Update,
            index: 0
        })
    );
    // Nothing ran while the first operation was still in flight.
    assert!(bench.catalog.ran().is_empty());

    let lists = bench.catalog.list_calls();
    bench.catalog.release_operations();
    let view = bench.until("the queue to drain", Bench::idle);
    assert_eq!(
        bench.catalog.ran(),
        [
            (OperationKind::Update, "Alpha.App".to_string()),
            (OperationKind::Update, "Beta.App".to_string()),
        ]
    );
    assert!(matches!(view.op.last, Some((_, OpResult::Succeeded))));
    // The finished batch started a check, which found nothing left to update.
    assert!(bench.catalog.list_calls() > lists);
    assert_eq!(view.check.count, Some(0));
    assert_eq!(view.visible_updatable_count, 0);

    bench.session.toggle_history_drawer();
    let history = bench.view().history;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].key.id, "Beta.App", "newest first");
    assert!(
        history
            .iter()
            .all(|entry| entry.outcome == HistoryOutcome::Succeeded)
    );
    let saved = bench.catalog.written(&history_path).unwrap();
    assert!(saved.contains("Alpha.App") && saved.contains("Beta.App"));
}

#[test]
fn cancelling_stops_the_running_operation_and_records_it_as_cancelled() {
    let a = summary("Alpha.App", true);
    let mut bench = Bench::loaded(vec![a.clone()]);
    bench.catalog.hold_operations();
    bench.session.enqueue(queued(OperationKind::Update, &a));
    bench.session.cancel_running();
    assert!(bench.view().op.cancelling);
    let view = bench.until("the cancel", Bench::idle);
    assert!(matches!(view.op.last, Some((_, OpResult::Cancelled))));
    assert!(!view.op.cancelling);
    assert_eq!(bench.catalog.cancel_requests(), 1);
    assert!(bench.catalog.ran().is_empty());
    // The package still has its update.
    assert_eq!(view.visible_updatable_count, 1);
    bench.session.toggle_history_drawer();
    assert_eq!(bench.view().history[0].outcome, HistoryOutcome::Cancelled);
}

#[test]
fn a_failed_operation_is_reported_and_the_queue_continues() {
    let (a, b) = (summary("Alpha.App", true), summary("Beta.App", true));
    let mut bench = Bench::loaded(vec![a.clone(), b.clone()]);
    bench.catalog.fail_operations_on("Alpha.App");
    bench.catalog.hold_operations();
    bench.session.enqueue(queued(OperationKind::Update, &a));
    bench.session.enqueue(queued(OperationKind::Update, &b));
    bench.catalog.release_operations();
    let view = bench.until("the queue to drain", Bench::idle);
    assert_eq!(
        bench.catalog.ran(),
        [(OperationKind::Update, "Beta.App".to_string())]
    );
    assert!(matches!(view.op.last, Some((ref op, OpResult::Succeeded)) if op.name == "Beta.App"));
    bench.session.toggle_history_drawer();
    let history = bench.view().history;
    let failed = history
        .iter()
        .find(|entry| entry.key.id == "Alpha.App")
        .unwrap();
    assert_eq!(failed.outcome, HistoryOutcome::Failed);
    assert!(failed.log.contains("could not be changed"));
    // Dismissing the banner clears the last result.
    bench.session.dismiss_notice();
    assert_eq!(bench.view().op.last, None);
}

#[test]
fn a_failed_first_load_says_why_and_a_failed_refresh_keeps_the_rows() {
    let mut cold = Bench::new(Vec::new());
    cold.catalog
        .fail_listing(CatalogError::Unavailable("no managers".into()));
    cold.session.ensure_loaded();
    let view = cold.until("the failure", |view| view.status != Status::Loading);
    assert_eq!(view.status, Status::Unavailable("no managers".into()));

    let mut warm = Bench::loaded(vec![summary("Alpha.App", false)]);
    warm.catalog
        .fail_listing(CatalogError::Failed("manager crashed".into()));
    warm.session.ensure_loaded();
    let view = warm.until("the refresh to fail", |view| view.notice.is_some());
    assert_eq!(view.status, Status::Ready);
    assert_eq!(view.total_installed, 1);
    assert_eq!(
        view.notice,
        Some(Notice::RefreshFailed("manager crashed".into()))
    );
    assert!(!view.refreshing);
}

#[test]
fn discover_lists_packages_that_are_not_installed_and_installs_one() {
    let installed = summary("Alpha.App", false);
    let mut found = summary("Git.Git", false);
    found.version = None;
    let mut bench = Bench::loaded(vec![installed.clone()]);
    bench
        .catalog
        .set_found(vec![installed.clone(), found.clone()]);
    bench
        .session
        .start_discover("  a  ".into(), Some("all".into()));
    let view = bench.until("the search", |view| {
        view.mode == ListMode::Discover && view.status == Status::Ready
    });
    assert_eq!(view.discover_query, "a");
    assert_eq!(view.discover_source, None);
    // "Alpha.App" also matches the query but is already installed.
    assert_eq!(view.packages.len(), 0);
    bench.session.start_discover("git".into(), None);
    let view = bench.until("the second search", |view| view.packages.len() == 1);
    assert!(!view.packages[0].installed);

    bench
        .session
        .enqueue(queued(OperationKind::Install, &found));
    bench.until("the install", Bench::idle);
    assert_eq!(
        bench.catalog.ran(),
        [(OperationKind::Install, "Git.Git".to_string())]
    );
    // The installed package leaves the Discover list and joins the Installed one.
    assert!(bench.view().packages.is_empty());
    bench.session.exit_discover();
    let view = bench.until("the installed list", |view| {
        view.mode == ListMode::Installed && view.total_installed == 2
    });
    assert!(view.packages.iter().any(|row| row.key.id == "Git.Git"));
    // An empty query does not start a search.
    bench.session.start_discover("   ".into(), None);
    assert_eq!(bench.view().mode, ListMode::Installed);
}

#[test]
fn export_writes_the_installed_packages_through_the_catalog() {
    let mut bench = Bench::loaded(vec![summary("Alpha.App", false), summary("Beta.App", true)]);
    let path = PathBuf::from("fake-export.json");
    bench.session.export(path.clone(), &bench.prefs.clone());
    let view = bench.until("the export", |view| view.notice.is_some());
    assert_eq!(
        view.notice,
        Some(Notice::Exported(2, path.display().to_string()))
    );
    let document = bench.catalog.written(&path).unwrap();
    assert!(document.contains("Alpha.App") && document.contains("Beta.App"));
    assert!(document.contains("2026-11-23T07:00:00.000-00:00"));
}

#[test]
fn shutdown_cancels_the_running_operation_and_drops_the_queue() {
    let (a, b) = (summary("Alpha.App", true), summary("Beta.App", true));
    let mut bench = Bench::loaded(vec![a.clone(), b.clone()]);
    bench.catalog.hold_operations();
    bench.session.enqueue(queued(OperationKind::Update, &a));
    bench.session.enqueue(queued(OperationKind::Update, &b));
    bench.session.shutdown();
    let view = bench.view();
    assert!(view.op.running.is_none());
    assert_eq!(view.op.pending, 0);
    assert_eq!(bench.catalog.cancel_requests(), 1);
    assert!(bench.catalog.ran().is_empty());
}

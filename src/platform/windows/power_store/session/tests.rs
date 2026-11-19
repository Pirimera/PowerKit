//! Session behaviour: list loading, selection, filters, queue and check scheduling.

use super::*;
use crate::power_store::ManagerKind;

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
        ..Session::default()
    };
    session.refresh_installed(true);
    assert!(session.refreshing);
    assert!(matches!(session.installed_status, Status::Ready));
    assert_eq!(session.installed_packages.len(), 1);
    // Drop the in-flight worker so this unit test does not wait on managers.
    session.installed_generation += 1;
    session.installed_receiver = None;
    session.refreshing = false;

    let mut cold = Session::default();
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
        ..Session::default()
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
        ..Session::default()
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
        ..Session::default()
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

#[test]
fn ensure_loaded_silently_refreshes_a_ready_cache() {
    let mut session = Session {
        installed_packages: vec![summary("Alpha.App", false)],
        installed_status: Status::Ready,
        ..Session::default()
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
    let mut session = Session::default();
    assert_eq!(session.update_count(&UpdateFilters::default()), None);
    session.store_updates(&[summary("A", true), summary("B", true), summary("C", false)]);
    let mut filters = UpdateFilters::default();
    assert_eq!(session.update_count(&filters), Some(2));
    filters.ignore(PackageKey::new(ManagerKind::WinGet, "winget", "A"));
    assert_eq!(session.update_count(&filters), Some(1));
}

#[test]
fn first_periodic_check_waits_for_the_startup_delay() {
    let mut session = Session::default();
    let start = Instant::now();
    session.tick(start, 60);
    assert!(!session.checking);
    assert_eq!(session.next_check, Some(start + FIRST_CHECK_DELAY));
}

#[test]
fn selection_accepts_only_listed_rows_and_select_all_respects_filters() {
    let mut session = Session::default();
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
    let mut session = Session::default();
    let own = summary(crate::power_store::OWN_PACKAGE_ID, true);
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
    let mut session = Session::default();
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
        crate::power_store::ProgressStage::Downloading,
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
    let mut session = Session::default();
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
    let mut session = Session::default();
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

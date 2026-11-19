//! Unit tests for PowerStore's package model.

use super::*;

fn key(id: &str) -> PackageKey {
    PackageKey::new(ManagerKind::WinGet, "winget", id)
}

#[test]
fn check_interval_steps_stay_in_range() {
    assert_eq!(step_check_minutes(DEFAULT_CHECK_MINUTES, true), 720);
    assert_eq!(step_check_minutes(DEFAULT_CHECK_MINUTES, false), 180);
    assert_eq!(
        step_check_minutes(MIN_CHECK_MINUTES, false),
        MIN_CHECK_MINUTES
    );
    assert_eq!(
        step_check_minutes(MAX_CHECK_MINUTES, true),
        MAX_CHECK_MINUTES
    );
    assert_eq!(step_check_minutes(100, true), 120);
    assert_eq!(step_check_minutes(100, false), 60);
}

#[test]
fn own_package_is_blocked_from_update_queue() {
    let mut queue = OperationQueue::default();
    queue.push(QueuedOperation {
        kind: OperationKind::Update,
        key: key(OWN_PACKAGE_ID),
        name: "PowerKit".into(),
        options: InstallOptions::default(),
    });
    queue.push(QueuedOperation {
        kind: OperationKind::Install,
        key: key("Git.Git"),
        name: "Git".into(),
        options: InstallOptions::default(),
    });
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.pop_front().unwrap().key.id, "Git.Git");
}

#[test]
fn own_package_guard_matches_plain_and_msix_listings_only() {
    let listed = |id: &str| PackageKey::new(ManagerKind::WinGet, "local", id);
    assert!(listed("PiriMedya.PowerKit").is_own_package());
    assert!(listed("MSIX\\PiriMedya.PowerKit_0.18.0.0_arm64__6fnnvtwr0r606").is_own_package());
    assert!(!listed("MSIX\\Pirimera.PowerKit.Shell_0.1.0.0_arm64__6fnnvtwr0r606").is_own_package());
    assert!(!PackageKey::new(ManagerKind::Npm, "npm", "PiriMedya.PowerKit").is_own_package());
}

#[test]
fn install_option_fields_follow_what_each_adapter_honors() {
    assert!(ManagerKind::WinGet.supports_version());
    assert!(ManagerKind::WinGet.supports_extra_args());
    assert!(ManagerKind::Scoop.supports_version());
    assert!(!ManagerKind::Scoop.supports_extra_args());
    assert!(ManagerKind::Chocolatey.supports_extra_args());
    assert!(!ManagerKind::Npm.supports_install_options());
    assert!(!ManagerKind::Vcpkg.supports_install_options());
    assert!(ManagerKind::Vcpkg.supports_default_triplet());
}

#[test]
fn the_same_package_and_kind_is_queued_once() {
    let mut queue = OperationQueue::default();
    let update = |id: &str| QueuedOperation {
        kind: OperationKind::Update,
        key: key(id),
        name: id.into(),
        options: InstallOptions::default(),
    };
    queue.push(update("Git.Git"));
    queue.push(update("Git.Git"));
    queue.push(QueuedOperation {
        kind: OperationKind::Uninstall,
        key: key("Git.Git"),
        name: "Git".into(),
        options: InstallOptions::default(),
    });
    assert_eq!(queue.len(), 2);
}

#[test]
fn remove_at_drops_a_waiting_queue_item() {
    let mut queue = OperationQueue::default();
    queue.push(QueuedOperation {
        kind: OperationKind::Install,
        key: key("A.A"),
        name: "A".into(),
        options: InstallOptions::default(),
    });
    queue.push(QueuedOperation {
        kind: OperationKind::Update,
        key: key("B.B"),
        name: "B".into(),
        options: InstallOptions::default(),
    });
    queue.push(QueuedOperation {
        kind: OperationKind::Uninstall,
        key: key("C.C"),
        name: "C".into(),
        options: InstallOptions::default(),
    });
    assert_eq!(queue.remove_at(1).unwrap().key.id, "B.B");
    assert_eq!(queue.len(), 2);
    assert!(queue.remove_at(9).is_none());
    let ids: Vec<_> = queue.iter().map(|item| item.key.id.as_str()).collect();
    assert_eq!(ids, ["A.A", "C.C"]);
}

#[test]
fn ignore_and_skip_filter_updates() {
    let mut filters = UpdateFilters::default();
    assert!(filters.ignore(key("Ignored.App")));
    assert!(filters.skip_version(key("Skipped.App"), "2.0.0"));
    let summary = |id: &str, installed: &str, available: &str, update_available| PackageSummary {
        key: key(id),
        name: id.into(),
        version: Some(installed.into()),
        available_version: Some(available.into()),
        update_available,
        scope: None,
        install_date: None,
        updated_at: None,
        icon: None,
    };
    let packages = [
        summary("Ignored.App", "1.0.0", "1.1.0", true),
        summary("Skipped.App", "1.0.0", "2.0.0", true),
        summary("Ready.App", "1.0.0", "1.2.0", true),
        summary(OWN_PACKAGE_ID, "0.1.0", "0.2.0", true),
        // Self-updating apps can run ahead of the catalog (Edge, OneDrive).
        summary("Ahead.App", "157.0.4341.60", "156.0.4314.58", false),
    ];
    let updatable = filter_updatable(&packages, &filters);
    assert_eq!(updatable.len(), 1);
    assert_eq!(updatable[0].key.id, "Ready.App");
}

#[test]
fn unified_query_pins_updates_on_top_and_sorts_dates_with_missing_last() {
    let mut prefs = PowerStorePreferences::default();
    prefs.filters.ignore(key("Ignored.App"));
    let make = |id: &str,
                name: &str,
                source: &str,
                update: bool,
                scope: Option<&str>,
                installed_on: Option<&str>,
                updated_on: Option<&str>| PackageSummary {
        key: PackageKey::new(ManagerKind::WinGet, source, id),
        name: name.into(),
        version: Some("1.0".into()),
        available_version: Some(if update { "2.0" } else { "1.0" }.into()),
        update_available: update,
        scope: scope.map(Into::into),
        install_date: installed_on.map(Into::into),
        updated_at: updated_on.map(Into::into),
        icon: None,
    };
    let packages = vec![
        make(
            "Zeta.Tool",
            "Zeta",
            "winget",
            false,
            Some("user"),
            Some("2026-01-10"),
            Some("2026-03-01"),
        ),
        make(
            "Beta.Update",
            "Beta",
            "msstore",
            true,
            Some("machine"),
            Some("2026-02-01"),
            Some("2026-11-10"),
        ),
        make("Alpha.Tool", "Alpha", "winget", false, None, None, None),
        make(
            "Alpha.Update",
            "Argo",
            "winget",
            true,
            Some("user"),
            Some("2026-05-01"),
            None,
        ),
        make(
            "Ignored.App",
            "Ignored",
            "winget",
            true,
            Some("user"),
            Some("2026-09-01"),
            None,
        ),
    ];

    // Default unified view: actionable updates first (sorted by name), then the rest by name.
    let mut query = ListViewQuery::default();
    let ids: Vec<&str> = query_packages(&packages, ListMode::Installed, &query, &prefs)
        .into_iter()
        .map(|package| package.key.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            "Alpha.Update",
            "Beta.Update",
            "Alpha.Tool",
            "Ignored.App",
            "Zeta.Tool"
        ]
    );

    // Sort by install date (newest first): updates stay on top, dated rows precede undated rows.
    query.sort_by = SortKey::InstalledDate;
    query.sort_ascending = false;
    let by_date: Vec<&str> = query_packages(&packages, ListMode::Installed, &query, &prefs)
        .into_iter()
        .map(|package| package.key.id.as_str())
        .collect();
    assert_eq!(
        by_date,
        vec![
            "Alpha.Update",
            "Beta.Update",
            "Ignored.App",
            "Zeta.Tool",
            "Alpha.Tool"
        ]
    );

    // Pinned status filter shows ignored / skipped entries; source + scope + text filters narrow.
    query.status = StatusFilter::Pinned;
    let pinned = query_packages(&packages, ListMode::Installed, &query, &prefs);
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].key.id, "Ignored.App");

    query.status = StatusFilter::All;
    query.sources = BTreeSet::from(["winget".to_string()]);
    query.scope = ScopeFilter::User;
    query.filter_text = "zeta".into();
    let narrowed = query_packages(&packages, ListMode::Installed, &query, &prefs);
    assert_eq!(narrowed.len(), 1);
    assert_eq!(narrowed[0].key.id, "Zeta.Tool");

    // Several sources combine as "any of".
    query.scope = ScopeFilter::All;
    query.filter_text.clear();
    query.sources = BTreeSet::from(["winget".to_string(), "msstore".to_string()]);
    let any_of = query_packages(&packages, ListMode::Installed, &query, &prefs);
    assert!(!any_of.is_empty());
    assert!(
        any_of
            .iter()
            .all(|package| matches!(package.key.source.as_str(), "winget" | "msstore"))
    );
}

#[test]
fn sort_choices_cover_both_directions_for_every_key() {
    for key in SortKey::ALL {
        for ascending in [true, false] {
            let index = sort_choice_index(key, ascending);
            assert_eq!(sort_choice_from_index(index), Some((key, ascending)));
        }
    }
    assert_eq!(sort_choice_index(SortKey::Name, true), 0);
    assert_eq!(sort_choice_index(SortKey::UpdatedDate, false), 7);
    assert_eq!(sort_choice_from_index(8), None);
}

#[test]
fn export_extras_round_trip() {
    let extras = ExportExtras {
        schema_version: ExportExtras::SCHEMA_VERSION,
        packages: vec![ExportPackage {
            key: PackageKey::new(ManagerKind::Scoop, SCOOP_SOURCE, "ripgrep"),
            options: InstallOptions {
                architecture: Some("arm64".into()),
                ..InstallOptions::default()
            },
        }],
    };
    let json = extras.to_json_object();
    assert!(json.contains("\"Manager\":\"scoop\""));
    assert_eq!(ExportExtras::parse_json_object(&json), Some(extras));
}

#[test]
fn scoop_export_json_parses_apps() {
    let text = r#"{
  "apps": [
{ "Name": "7zip", "Source": "main", "Version": "24.08", "Info": "" },
{ "Name": "ripgrep", "Bucket": "main", "Version": "14.1.0" },
{ "Name": "microsoft-jdk", "Source": "java", "Version": "25.0.5" }
  ]
}"#;
    let packages = parse_scoop_export(text).expect("parse");
    assert_eq!(packages.len(), 3);
    assert_eq!(packages[0].key.manager, ManagerKind::Scoop);
    assert_eq!(packages[0].key.source, SCOOP_SOURCE);
    assert_eq!(packages[0].key.id, "7zip");
    assert_eq!(packages[0].name, "7zip");
    assert_eq!(packages[0].version.as_deref(), Some("24.08"));
    assert_eq!(packages[1].key.id, "ripgrep");
    assert_eq!(packages[2].key.source, SCOOP_SOURCE);
    assert_eq!(packages[2].key.id, "java/microsoft-jdk");
    assert_eq!(packages[2].name, "microsoft-jdk");
}

#[test]
fn scoop_ids_keep_the_bucket_only_outside_main() {
    assert_eq!(scoop_package_id("main", "git"), "git");
    assert_eq!(scoop_package_id("", "git"), "git");
    assert_eq!(
        scoop_package_id("java", "microsoft-jdk"),
        "java/microsoft-jdk"
    );
    assert_eq!(split_scoop_id("git"), ("main", "git"));
    assert_eq!(
        split_scoop_id("java/microsoft-jdk"),
        ("java", "microsoft-jdk")
    );
}

#[test]
fn scoop_keys_saved_with_the_bucket_as_source_still_load() {
    let main = parse_package_key("scoop|main|7zip").expect("main");
    assert_eq!(
        main,
        PackageKey::new(ManagerKind::Scoop, SCOOP_SOURCE, "7zip")
    );
    let java = parse_package_key("scoop|java|microsoft-jdk").expect("java");
    assert_eq!(
        java,
        PackageKey::new(ManagerKind::Scoop, SCOOP_SOURCE, "java/microsoft-jdk")
    );
    let winget = parse_package_key("winget|winget|Git.Git").expect("winget");
    assert_eq!(winget.source, "winget");
}

#[test]
fn choco_limit_and_outdated_rows_parse() {
    let listed = "git|2.45.1\n7zip|24.8\nChocolatey v2.2.2\n";
    assert_eq!(
        parse_choco_limit_rows(listed),
        vec![
            ("git".into(), "2.45.1".into()),
            ("7zip".into(), "24.8".into())
        ]
    );
    let outdated = "git|2.45.1|2.46.0|false\n";
    assert_eq!(
        parse_choco_outdated_rows(outdated),
        vec![("git".into(), "2.45.1".into(), "2.46.0".into())]
    );
    assert!(version_looks_newer("2.46.0", "2.45.1"));
    assert!(!version_looks_newer("1.0.0", "1.0.0"));
}

#[test]
fn phase3_cli_parsers_accept_fixtures() {
    let npm_list = r#"{"dependencies":{"typescript":{"version":"5.4.0"}}}"#;
    assert_eq!(
        parse_npm_list_json(npm_list),
        vec![("typescript".into(), "5.4.0".into())]
    );
    let npm_out = r#"{"typescript":{"current":"5.4.0","wanted":"5.4.5","latest":"5.5.0"}}"#;
    assert_eq!(
        parse_npm_outdated_json(npm_out),
        vec![("typescript".into(), "5.5.0".into())]
    );
    let pip = r#"[{"name":"pip","version":"24.0"},{"name":"wheel","version":"0.43.0"}]"#;
    assert_eq!(parse_pip_list_json(pip).len(), 2);
    let pip_out =
        r#"[{"name":"pip","version":"24.0","latest_version":"24.2","latest_filetype":"wheel"}]"#;
    assert_eq!(
        parse_pip_outdated_json(pip_out),
        vec![("pip".into(), "24.0".into(), "24.2".into())]
    );
    let cargo = "ripgrep v14.1.0:\n    rg\nfd-find v10.1.0:\n    fd\n";
    assert_eq!(
        parse_cargo_install_list(cargo),
        vec![
            ("ripgrep".into(), "14.1.0".into()),
            ("fd-find".into(), "10.1.0".into())
        ]
    );
    let cargo_search = "serde = \"1.0.210\"    # serialization\n... and 3 more crates\n";
    assert_eq!(
        parse_cargo_search(cargo_search)[0],
        ("serde".into(), "1.0.210".into(), "serialization".into())
    );
    let dotnet = "Package Id      Version      Commands\n--------------------------------------\ndotnetsay       2.1.0        dotnetsay\n";
    assert_eq!(
        parse_dotnet_tool_list(dotnet),
        vec![("dotnetsay".into(), "2.1.0".into())]
    );
    let ps = r#"[{"Name":"PSReadLine","Version":"2.3.5"}]"#;
    assert_eq!(
        parse_powershell_modules_json(ps),
        vec![("PSReadLine".into(), "2.3.5".into())]
    );
    let vcpkg = "fmt:x64-windows              9.0.0              Formatting library\n";
    assert_eq!(
        parse_vcpkg_list(vcpkg),
        vec![(
            "fmt:x64-windows".into(),
            "9.0.0".into(),
            "Formatting library".into()
        )]
    );
    assert!(ManagerKind::Npm.phase3());
    assert!(!ManagerKind::Npm.phase2());
}

#[test]
fn manager_defaults_are_not_stored_and_invalid_settings_are_rejected() {
    let mut prefs = PowerStorePreferences::default();
    assert!(prefs.manager(ManagerKind::Npm).enabled);
    assert!(prefs.set_manager(
        ManagerKind::Npm,
        ManagerSettings {
            enabled: false,
            ..ManagerSettings::default()
        }
    ));
    assert!(!prefs.manager(ManagerKind::Npm).enabled);
    assert!(prefs.set_manager(ManagerKind::Npm, ManagerSettings::default()));
    assert!(prefs.managers.is_empty());

    let scope = |value: &str| ManagerSettings {
        scope: Some(value.into()),
        ..ManagerSettings::default()
    };
    let executable = |value: &str| ManagerSettings {
        executable: Some(value.into()),
        ..ManagerSettings::default()
    };
    assert!(!prefs.set_manager(ManagerKind::WinGet, scope("root")));
    assert!(!prefs.set_manager(ManagerKind::Npm, scope("user")));
    assert!(!prefs.set_manager(ManagerKind::WinGet, executable("winget.exe")));
    assert!(!prefs.set_manager(ManagerKind::Npm, executable("a\nb")));
    assert!(!prefs.set_manager(ManagerKind::Npm, executable(&"x".repeat(2000))));
    assert!(prefs.set_manager(ManagerKind::Npm, executable("  C:\\node\\npm.cmd ")));
    assert_eq!(
        prefs.manager(ManagerKind::Npm).executable.as_deref(),
        Some("C:\\node\\npm.cmd")
    );
}

#[test]
fn manager_default_scope_fills_operations_but_not_the_options_editor() {
    let mut prefs = PowerStorePreferences::default();
    let git = PackageKey::new(ManagerKind::WinGet, "winget", "Git.Git");
    let code = PackageKey::new(ManagerKind::WinGet, "winget", "Microsoft.VisualStudioCode");
    assert!(prefs.set_manager(
        ManagerKind::WinGet,
        ManagerSettings {
            scope: Some("machine".into()),
            ..ManagerSettings::default()
        }
    ));
    assert!(prefs.set_options(
        code.clone(),
        InstallOptions {
            scope: Some("user".into()),
            ..InstallOptions::default()
        }
    ));
    assert_eq!(
        prefs.operation_options_for(&git).scope.as_deref(),
        Some("machine")
    );
    assert_eq!(
        prefs.operation_options_for(&code).scope.as_deref(),
        Some("user")
    );
    assert_eq!(prefs.options_for(&git).scope, None);
}

#[test]
fn manager_default_architecture_and_triplet_follow_their_manager() {
    let mut prefs = PowerStorePreferences::default();
    let git = PackageKey::new(ManagerKind::WinGet, "winget", "Git.Git");
    let code = PackageKey::new(ManagerKind::WinGet, "winget", "Microsoft.VisualStudioCode");
    let architecture = |value: &str| ManagerSettings {
        architecture: Some(value.into()),
        ..ManagerSettings::default()
    };
    let triplet = |value: &str| ManagerSettings {
        triplet: Some(value.into()),
        ..ManagerSettings::default()
    };
    assert!(!prefs.set_manager(ManagerKind::WinGet, architecture("sparc")));
    assert!(!prefs.set_manager(ManagerKind::Npm, architecture("arm64")));
    assert!(!prefs.set_manager(ManagerKind::WinGet, triplet("arm64-windows")));
    assert!(!prefs.set_manager(ManagerKind::Vcpkg, triplet("linux")));
    assert!(!prefs.set_manager(ManagerKind::Vcpkg, architecture("arm64")));
    assert!(prefs.set_manager(ManagerKind::Vcpkg, triplet("arm64-windows")));
    assert!(prefs.set_manager(ManagerKind::WinGet, architecture("arm64")));
    assert!(prefs.set_options(
        code.clone(),
        InstallOptions {
            architecture: Some("x64".into()),
            ..InstallOptions::default()
        }
    ));
    assert_eq!(
        prefs.operation_options_for(&git).architecture.as_deref(),
        Some("arm64")
    );
    assert_eq!(
        prefs.operation_options_for(&code).architecture.as_deref(),
        Some("x64")
    );
    assert_eq!(prefs.options_for(&git).architecture, None);

    let mut body = String::new();
    prefs.append_settings(&mut body);
    assert!(body.contains("power_store_manager=vcpkg|1|||arm64-windows|\n"));
    assert!(body.contains("power_store_manager=winget|1||arm64||\n"));
    let (kind, parsed) = parse_manager_line("vcpkg|1|||arm64-windows|").unwrap();
    assert_eq!(kind, ManagerKind::Vcpkg);
    assert_eq!(parsed.triplet.as_deref(), Some("arm64-windows"));
    // Lines written before architecture and triplet existed still load.
    let (kind, legacy) = parse_manager_line("winget|1|machine|").unwrap();
    assert_eq!(kind, ManagerKind::WinGet);
    assert_eq!(legacy.scope.as_deref(), Some("machine"));
    assert_eq!(legacy.architecture, None);
    assert!(parse_manager_line("winget|1|machine|arm64").is_none());
}

#[test]
fn manager_install_hints_exist_except_for_builtin_windows_powershell() {
    for kind in [
        ManagerKind::WinGet,
        ManagerKind::Scoop,
        ManagerKind::Chocolatey,
        ManagerKind::Npm,
        ManagerKind::Pip,
        ManagerKind::Cargo,
        ManagerKind::DotnetTool,
        ManagerKind::PowerShell7,
        ManagerKind::Vcpkg,
    ] {
        assert!(
            kind.install_hint().is_some_and(|hint| !hint.contains('\n')),
            "{kind:?}"
        );
    }
    assert_eq!(ManagerKind::PowerShell.install_hint(), None);
}

#[test]
fn clamp_check_minutes_bounds() {
    assert_eq!(clamp_check_minutes(1), MIN_CHECK_MINUTES);
    assert_eq!(clamp_check_minutes(60), 60);
    assert_eq!(clamp_check_minutes(u32::MAX), MAX_CHECK_MINUTES);
}

#[test]
fn options_line_round_trips_and_rejects_invalid_values() {
    let mut prefs = PowerStorePreferences::default();
    let options = InstallOptions {
        scope: Some(" user ".into()),
        architecture: Some("arm64".into()),
        version: Some("1.2.3".into()),
        extra_args: Some("--flag=a|b {x}".into()),
    };
    assert!(prefs.set_options(key("Contoso.App"), options));
    let mut body = String::new();
    prefs.append_settings(&mut body);
    let line = body
        .lines()
        .find_map(|line| line.strip_prefix("power_store_option="))
        .expect("options line");
    let (parsed_key, parsed) = parse_options_line(line).expect("parse");
    assert_eq!(parsed_key, key("Contoso.App"));
    assert_eq!(parsed.scope.as_deref(), Some("user"));
    assert_eq!(parsed.extra_args.as_deref(), Some("--flag=a|b {x}"));
    assert!(!prefs.set_options(
        key("Bad.Scope"),
        InstallOptions {
            scope: Some("everyone".into()),
            ..InstallOptions::default()
        }
    ));
    assert!(!prefs.set_options(
        key("Bad.Args"),
        InstallOptions {
            extra_args: Some("a\nb".into()),
            ..InstallOptions::default()
        }
    ));
    assert!(prefs.set_options(key("Contoso.App"), InstallOptions::default()));
    assert!(prefs.options.is_empty());
    assert_eq!(parse_options_line("winget|winget|Only.Key"), None);
}

#[test]
fn history_is_bounded_truncates_logs_and_skips_bad_lines() {
    let entry = |index: usize, log: String| HistoryEntry {
        time: format!("2026-11-16 10:{index:02}"),
        kind: OperationKind::Install,
        key: key(&format!("App.{index}")),
        name: format!("App\t{index}"),
        outcome: HistoryOutcome::Failed,
        log,
    };
    let mut history = History::default();
    for index in 0..MAX_HISTORY_OPERATIONS + 5 {
        history.push(entry(index, "ğ".repeat(MAX_HISTORY_LOG_BYTES)));
    }
    assert_eq!(history.len(), MAX_HISTORY_OPERATIONS);
    let newest = history.newest_first().next().expect("newest");
    assert_eq!(newest.key.id, format!("App.{}", MAX_HISTORY_OPERATIONS + 4));
    assert!(newest.log.len() <= MAX_HISTORY_LOG_BYTES);
    let mut text = history.serialize();
    text.push_str("garbage line\nonly\ttwo\n");
    assert_eq!(History::parse(&text), history);
    assert_eq!(History::parse("unversioned\n").len(), 0);
}

/// Writes an export for `winget import --no-upgrade` to accept or reject; re-run
/// after WinGet updates in case its schema validation tightens.
#[test]
#[ignore = "requires POWERKIT_EXPORT_FIXTURE and POWERKIT_EXPORT_IDS"]
fn writes_winget_import_fixture() {
    let path = std::env::var("POWERKIT_EXPORT_FIXTURE").unwrap();
    let ids = std::env::var("POWERKIT_EXPORT_IDS").unwrap();
    let mut packages: Vec<ExportPackage> = ids
        .split(',')
        .map(|id| ExportPackage {
            key: key(id),
            options: InstallOptions::default(),
        })
        .collect();
    if let Some(first) = packages.first_mut() {
        first.options.scope = Some("user".into());
    }
    let document = export_document(&packages, "2026-11-16T12:00:00.000-00:00");
    assert_eq!(parse_import(&document).unwrap(), packages);
    std::fs::write(path, document).unwrap();
}

#[test]
fn export_document_imports_back_with_options() {
    let packages = vec![
        ExportPackage {
            key: key("Git.Git"),
            options: InstallOptions::default(),
        },
        ExportPackage {
            key: key("PowerKit"),
            options: InstallOptions {
                extra_args: Some("--x \"{y}\" ]".into()),
                ..InstallOptions::default()
            },
        },
        ExportPackage {
            key: PackageKey::new(ManagerKind::WinGet, "msstore", "9NBLGGH4NNS1"),
            options: InstallOptions::default(),
        },
    ];
    let document = export_document(&packages, "2026-11-16T10:00:00.000-00:00");
    assert!(document.contains("\"Name\": \"msstore\""));
    assert_eq!(parse_import(&document), Some(packages));
    assert_eq!(parse_import("{\"NoSources\": []}"), None);
}

#[test]
fn progress_percent_is_clamped_and_unknown_without_a_value() {
    let percent = |fraction| OperationProgress::new(ProgressStage::Downloading, fraction).percent;
    assert_eq!(percent(0.0), None);
    assert_eq!(percent(f64::NAN), None);
    assert_eq!(percent(-0.5), None);
    assert_eq!(percent(0.424), Some(42));
    assert_eq!(percent(7.0), Some(100));
    for stage in [
        ProgressStage::Queued,
        ProgressStage::Downloading,
        ProgressStage::Installing,
        ProgressStage::Uninstalling,
        ProgressStage::Finishing,
    ] {
        assert_eq!(ProgressStage::parse(stage.as_str()), Some(stage));
    }
}

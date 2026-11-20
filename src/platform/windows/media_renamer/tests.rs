use super::*;
use crate::media_renamer::bmff::fixtures;
use std::time::Instant;

const SETTLE: Duration = Duration::from_secs(20);

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "powerkit-rename-session-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn apple_movie(path: &Path, created: &str) {
    std::fs::write(
        path,
        fixtures::movie(&[
            fixtures::mvhd(1),
            fixtures::apple_meta(&[("creationdate", created)]),
        ]),
    )
    .unwrap();
}
/// Polls like the UI thread until no scan or work job remains.
fn settle(session: &mut Session) {
    let start = Instant::now();
    while session.scan.is_some() || session.work.is_some() {
        assert!(start.elapsed() < SETTLE, "session did not settle");
        session.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn folder_preview_applies_with_companions_and_collisions_then_undoes() {
    let dir = temp("flow");
    let media = dir.join("media");
    std::fs::create_dir_all(&media).unwrap();
    apple_movie(&media.join("IMG_0001.MOV"), "2024-05-01T15:00:00+0300");
    apple_movie(&media.join("IMG_0002.MOV"), "2024-05-01T15:00:00+0300");
    std::fs::write(media.join("IMG_0001.xmp"), b"synthetic sidecar").unwrap();
    std::fs::write(media.join("notes.txt"), b"not media").unwrap();
    let original = names(&media);

    let prefs = Preferences::default();
    let mut session = Session::new(0, &prefs);
    session.load_history(dir.join(crate::media_renamer::history::FILE_NAME));
    settle(&mut session);
    session.add(vec![media.clone()]);
    settle(&mut session);

    let view = session.snapshot(&prefs);
    assert_eq!(view.summary.groups, 2);
    assert_eq!(view.summary.ready, 2);
    assert_eq!(view.skipped, 1);
    assert!(view.can_apply);
    assert!(view.rows.iter().any(|row| row.collided));
    assert!(session.has_unapplied());
    assert_eq!(session.tray_status(), TrayStatus::Ready(2));

    session.apply();
    settle(&mut session);
    let renamed = names(&media);
    assert_eq!(
        renamed,
        [
            "2024-05-01_15-00-00.MOV",
            "2024-05-01_15-00-00.xmp",
            "2024-05-01_15-00-00_1.MOV",
            "notes.txt",
        ]
    );
    let view = session.snapshot(&prefs);
    assert!(matches!(
        view.notice,
        Some(Notice::Applied { renamed: 3, ref failures, cancelled: false }) if failures.is_empty()
    ));
    assert_eq!(view.summary.ready, 0);
    assert!(!session.has_unapplied());
    let id = session.history[0].id;
    assert!(session.history[0].undoable());

    session.undo(id);
    settle(&mut session);
    assert_eq!(names(&media), original);
    assert!(matches!(
        session.snapshot(&prefs).notice,
        Some(Notice::Undone { restored: 3, skipped: 0, ref failures }) if failures.is_empty()
    ));

    let mut reloaded = Session::new(0, &prefs);
    reloaded.load_history(dir.join(crate::media_renamer::history::FILE_NAME));
    settle(&mut reloaded);
    assert_eq!(reloaded.history.len(), 2);
    assert!(!reloaded.history.iter().any(Batch::undoable));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stale_preview_refuses_apply_until_refreshed() {
    let dir = temp("refresh");
    apple_movie(&dir.join("a.mov"), "2024-05-01T10:00:00+0000");
    apple_movie(&dir.join("b.mov"), "2024-05-02T10:00:00+0000");
    let prefs = Preferences::default();
    let mut session = Session::new(0, &prefs);
    session.add(vec![dir.join("a.mov"), dir.join("b.mov")]);
    settle(&mut session);
    std::fs::remove_file(dir.join("a.mov")).unwrap();
    apple_movie(&dir.join("b.mov"), "2024-06-01T10:00:00+0000");

    session.apply();
    settle(&mut session);
    assert!(matches!(
        session.snapshot(&prefs).notice,
        Some(Notice::Stale)
    ));
    assert_eq!(names(&dir), ["b.mov"]);

    session.act(Action::Refresh);
    settle(&mut session);
    assert_eq!(session.snapshot(&prefs).summary.groups, 1);
    session.apply();
    settle(&mut session);
    assert_eq!(names(&dir), ["2024-06-01_10-00-00.mov"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn deselected_rows_stay_put_and_the_filter_only_hides_rows() {
    let dir = temp("select");
    apple_movie(&dir.join("a.mov"), "2024-05-01T10:00:00+0000");
    apple_movie(&dir.join("b.mov"), "2024-05-02T10:00:00+0000");
    let prefs = Preferences::default();
    let mut session = Session::new(0, &prefs);
    session.add(vec![dir.join("a.mov"), dir.join("b.mov")]);
    settle(&mut session);
    let first = session
        .rows
        .iter()
        .find(|row| file_name(&session.groups[row.group].members[0].path) == "a.mov")
        .unwrap()
        .group;
    session.act(Action::SetSelected(first, false));
    session.act(Action::SetFilterText("b.mov".into()));
    let view = session.snapshot(&prefs);
    assert_eq!(view.visible, 1);
    assert_eq!(view.filter_counts[StateFilter::All.index()], 2);
    assert_eq!(view.filter_counts[StateFilter::Ready.index()], 2);
    assert_eq!(view.summary.ready, 1);
    session.act(Action::SetStateFilter(StateFilter::Problem));
    assert_eq!(session.snapshot(&prefs).visible, 0);
    session.act(Action::ClearFilter);
    assert_eq!(session.snapshot(&prefs).visible, 2);
    session.apply();
    settle(&mut session);
    assert_eq!(names(&dir), ["2024-05-02_10-00-00.mov", "a.mov"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn same_stem_jpeg_and_raw_added_one_after_another_form_one_group_without_skips() {
    let dir = temp("pair");
    std::fs::write(dir.join("IMG_0001.jpg"), b"synthetic jpeg bytes").unwrap();
    std::fs::write(dir.join("IMG_0001.ARW"), b"synthetic raw bytes").unwrap();
    let prefs = Preferences::default();
    let mut session = Session::new(0, &prefs);
    session.add(vec![dir.join("IMG_0001.jpg")]);
    settle(&mut session);
    session.add(vec![dir.join("IMG_0001.ARW")]);
    settle(&mut session);
    let view = session.snapshot(&prefs);
    assert_eq!(
        view.skipped, 0,
        "a RAW next to its JPEG must not be skipped"
    );
    assert_eq!(view.summary.groups, 1);
    assert_eq!(view.visible, 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn details_read_metadata_for_media_files_only_and_stop_when_closed() {
    let dir = temp("metadata");
    apple_movie(&dir.join("IMG_0001.MOV"), "2024-05-01T15:00:00+0300");
    std::fs::write(dir.join("IMG_0001.xmp"), b"synthetic sidecar").unwrap();
    let prefs = Preferences::default();
    let mut session = Session::new(0, &prefs);
    session.add(vec![dir.clone()]);
    settle(&mut session);

    session.act(Action::OpenDetails(0));
    let start = Instant::now();
    while session.metadata.as_ref().is_some_and(|m| m.job.is_some()) {
        assert!(start.elapsed() < SETTLE, "metadata was not read");
        session.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    let DrawerView::Details(details) = session.snapshot(&prefs).drawer else {
        panic!("details drawer should be open");
    };
    assert_eq!(details.metadata.len(), 1, "the sidecar is not read");
    assert_eq!(details.metadata[0].name, "IMG_0001.MOV");
    assert!(matches!(
        details.metadata[0].metadata,
        MetadataView::Tags(_)
    ));

    session.act(Action::CloseDrawer);
    assert!(session.metadata.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

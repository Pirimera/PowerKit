//! Groups (Live Photo pairs, sidecars), deterministic collision planning and commit ordering.
use super::{
    Kind,
    template::{self, Fields, MAX_NAME_UNITS, Template},
    time::{Capture, TimeSource, WallClock},
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// Collision suffix room reserved when capping the stem (`_457_9999`).
const SUFFIX_ROOM: usize = 9;
const MAX_COUNTER: usize = 9999;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    pub capture: Option<Capture>,
    pub make: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Probe {
    Pending,
    Read(Metadata),
    OnlineOnly,
    Unreadable,
    CodecMissing(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub path: PathBuf,
    pub kind: Kind,
    /// Original stem, exactly as on disk.
    pub stem: String,
    /// Everything after the stem: `.JPG`, `.CR3.xmp`.
    pub suffix: String,
    pub size: u64,
    /// Last-write time as UTC FILETIME ticks, checked again before any rename.
    pub modified: i64,
    /// Last-write time on the local calendar, for the missing-date fallback.
    pub modified_local: Option<WallClock>,
    /// Added automatically because it shares the stem of an added file.
    pub companion: bool,
    pub probe: Probe,
}
/// Stem, suffix and kind of a supported file name; `IMG.CR3.xmp` has stem `IMG`.
pub fn split_name(file_name: &str) -> Option<(String, String, Kind)> {
    let (rest, extension) = file_name.rsplit_once('.')?;
    let kind = Kind::from_extension(extension)?;
    if rest.is_empty() {
        return None;
    }
    if kind == Kind::Sidecar
        && extension.eq_ignore_ascii_case("xmp")
        && let Some((inner, media)) = rest.rsplit_once('.')
        && !inner.is_empty()
        && Kind::from_extension(media).is_some_and(Kind::is_media)
    {
        return Some((inner.into(), file_name[inner.len()..].into(), kind));
    }
    Some((rest.into(), file_name[rest.len()..].into(), kind))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub dir: PathBuf,
    pub members: Vec<Member>,
}
fn key(dir: &Path, stem: &str) -> (String, String) {
    (dir.to_string_lossy().to_lowercase(), stem.to_lowercase())
}
impl Group {
    pub fn key(&self) -> (String, String) {
        key(&self.dir, &self.members[0].stem)
    }
    /// The member whose metadata names the group: still images with a capture time first.
    pub fn primary(&self) -> usize {
        let mut best = 0;
        let mut best_rank = (true, u8::MAX);
        for (index, member) in self.members.iter().enumerate() {
            if !member.kind.is_media() {
                continue;
            }
            let has_time = matches!(&member.probe, Probe::Read(m) if m.capture.is_some());
            let rank = (!has_time, member.kind.rank());
            if rank < best_rank {
                best = index;
                best_rank = rank;
            }
        }
        best
    }
    pub fn stem(&self) -> &str {
        &self.members[self.primary()].stem
    }
}
/// Groups members by folder and case-insensitive stem, keeping first-seen order.
/// Groups without a media file (a lone `.xmp`) are dropped.
pub fn group(members: Vec<Member>) -> Vec<Group> {
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut groups: Vec<Group> = Vec::new();
    for member in members {
        let dir = member
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let slot = *index.entry(key(&dir, &member.stem)).or_insert_with(|| {
            groups.push(Group {
                dir,
                members: Vec::new(),
            });
            groups.len() - 1
        });
        let paths = &groups[slot].members;
        if !paths.iter().any(|m| {
            m.path
                .as_os_str()
                .eq_ignore_ascii_case(member.path.as_os_str())
        }) {
            groups[slot].members.push(member);
        }
    }
    groups.retain(|group| group.members.iter().any(|m| m.kind.is_media()));
    groups
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Pending,
    Ready,
    Unchanged,
    NoDate,
    OnlineOnly,
    Unreadable,
    CodecMissing(&'static str),
    Conflict,
}
impl State {
    pub fn renamable(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    pub template: &'a Template,
    pub mtime_fallback: bool,
    pub lowercase_ext: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub group: usize,
    pub selected: bool,
    pub state: State,
    pub capture: Option<Capture>,
    /// New file name for every member, in member order; empty when not renamable.
    pub targets: Vec<PathBuf>,
    pub collided: bool,
}
impl Row {
    #[cfg(test)]
    pub fn new_stem<'a>(&'a self, groups: &'a [Group]) -> Option<&'a str> {
        let target = self.targets.first()?.file_name()?.to_str()?;
        let suffix = &groups[self.group].members[0].suffix;
        Some(&target[..target.len().saturating_sub(suffix.len())])
    }
}

fn effective(group: &Group, options: &Options) -> (Option<Capture>, State) {
    if group.members.iter().any(|m| m.probe == Probe::Pending) {
        return (None, State::Pending);
    }
    if group.members.iter().any(|m| m.probe == Probe::OnlineOnly) {
        return (None, State::OnlineOnly);
    }
    let primary = &group.members[group.primary()];
    let metadata = match &primary.probe {
        Probe::Read(metadata) => metadata,
        Probe::CodecMissing(name) => return (None, State::CodecMissing(name)),
        _ => return (None, State::Unreadable),
    };
    if let Some(capture) = metadata.capture {
        return (Some(capture), State::Ready);
    }
    let fallback = options
        .mtime_fallback
        .then_some(primary.modified_local)
        .flatten()
        .map(|time| Capture {
            time,
            offset: None,
            source: TimeSource::Modified,
        });
    match fallback {
        Some(capture) => (Some(capture), State::Ready),
        None if !options.template.needs_time() => (None, State::Ready),
        None => (None, State::NoDate),
    }
}
fn lower(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}
fn suffix(member: &Member, options: &Options) -> String {
    if options.lowercase_ext {
        member.suffix.to_lowercase()
    } else {
        member.suffix.clone()
    }
}

/// Plans new names in preview order (folder, capture time, original name).
///
/// `selected` is per group. Unselected and failed groups keep their names and stay
/// occupied; selected renamable groups free their current names for later rows.
/// `on_disk` answers whether a path exists before the batch.
pub fn plan(
    groups: &[Group],
    selected: &[bool],
    options: &Options,
    mut on_disk: impl FnMut(&Path) -> bool,
) -> Vec<Row> {
    let mut rows: Vec<Row> = groups
        .iter()
        .enumerate()
        .map(|(index, group)| {
            let (capture, state) = effective(group, options);
            Row {
                group: index,
                selected: selected.get(index).copied().unwrap_or(true),
                state,
                capture,
                targets: Vec::new(),
                collided: false,
            }
        })
        .collect();
    rows.sort_by_cached_key(|row| {
        let group = &groups[row.group];
        (
            lower(&group.dir),
            row.capture.is_none(),
            row.capture.map(|c| c.time),
            group.stem().to_lowercase(),
            row.group,
        )
    });
    let mut freed: HashSet<String> = HashSet::new();
    for row in rows.iter().filter(|r| r.selected && r.state.renamable()) {
        freed.extend(groups[row.group].members.iter().map(|m| lower(&m.path)));
    }
    let mut reserved: HashSet<String> = HashSet::new();
    let mut counter = 0;
    let mut deferred = Vec::new();
    for pass in [true, false] {
        for (position, row) in rows.iter_mut().enumerate() {
            if !row.state.renamable() {
                continue;
            }
            if row.selected != pass {
                continue;
            }
            if pass {
                counter += 1;
            }
            let number = if pass {
                counter
            } else {
                deferred.push(position);
                counter + deferred.len()
            };
            let group = &groups[row.group];
            let primary = &group.members[group.primary()];
            let metadata = match &primary.probe {
                Probe::Read(metadata) => Some(metadata),
                _ => None,
            };
            let raw = options.template.render(&Fields {
                time: row.capture.as_ref().map(|c| &c.time),
                make: metadata.and_then(|m| m.make.as_deref()),
                model: metadata.and_then(|m| m.model.as_deref()),
                original: group.stem(),
                counter: number,
            });
            let suffixes: Vec<String> = group.members.iter().map(|m| suffix(m, options)).collect();
            let longest = suffixes
                .iter()
                .map(|s| template::units(s))
                .max()
                .unwrap_or(0);
            let cap = MAX_NAME_UNITS.saturating_sub(longest + SUFFIX_ROOM).max(1);
            let base = template::sanitize_stem(&raw, cap);
            let millis = (!options.template.has_millis())
                .then(|| row.capture.and_then(|c| c.time.millis))
                .flatten();
            let mut free = |stem: &str| {
                suffixes.iter().all(|suffix| {
                    let path = group.dir.join(format!("{stem}{suffix}"));
                    let key = lower(&path);
                    !reserved.contains(&key) && (freed.contains(&key) || !on_disk(&path))
                })
            };
            let chosen = if free(&base) {
                Some((base, false))
            } else {
                let stepped = millis.map(|m| format!("{base}_{m:03}"));
                match stepped {
                    Some(stem) if free(&stem) => Some((stem, true)),
                    _ => {
                        let root = stepped.unwrap_or(base);
                        (1..=MAX_COUNTER)
                            .map(|n| format!("{root}_{n}"))
                            .find(|stem| free(stem))
                            .map(|stem| (stem, true))
                    }
                }
            };
            let Some((stem, collided)) = chosen else {
                row.state = State::Conflict;
                continue;
            };
            row.collided = collided;
            row.targets = suffixes
                .iter()
                .map(|suffix| group.dir.join(format!("{stem}{suffix}")))
                .collect();
            reserved.extend(row.targets.iter().map(|path| lower(path)));
            let unchanged = group
                .members
                .iter()
                .zip(&row.targets)
                .all(|(member, target)| member.path.file_name() == target.file_name());
            if unchanged {
                row.state = State::Unchanged;
            }
        }
    }
    rows
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub groups: usize,
    pub files: usize,
    pub ready: usize,
    pub unchanged: usize,
    pub no_date: usize,
    pub collided: usize,
    pub failed: usize,
    pub pending: usize,
}
pub fn summary(groups: &[Group], rows: &[Row]) -> Summary {
    let mut summary = Summary {
        groups: rows.len(),
        files: groups.iter().map(|g| g.members.len()).sum(),
        ..Summary::default()
    };
    for row in rows {
        match row.state {
            State::Ready if row.selected => summary.ready += 1,
            State::Ready => {}
            State::Unchanged => summary.unchanged += 1,
            State::NoDate => summary.no_date += 1,
            State::Pending => summary.pending += 1,
            State::OnlineOnly | State::Unreadable | State::CodecMissing(_) | State::Conflict => {
                summary.failed += 1
            }
        }
        if row.collided && row.state == State::Ready {
            summary.collided += 1;
        }
    }
    summary
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: PathBuf,
    pub to: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// Index into the input moves.
    pub index: usize,
    pub from: PathBuf,
    pub to: PathBuf,
}
/// Orders renames so no step lands on a name another move still has to vacate.
/// Cycles (swaps) move one file to `temp(index)` first and finish it last.
pub fn order(moves: &[Move], mut temp: impl FnMut(usize) -> PathBuf) -> Vec<Step> {
    let sources: HashMap<String, usize> = moves
        .iter()
        .enumerate()
        .map(|(index, m)| (lower(&m.from), index))
        .collect();
    let mut current: Vec<PathBuf> = moves.iter().map(|m| m.from.clone()).collect();
    let mut vacated = vec![false; moves.len()];
    // 0 pending, 1 visiting, 2 done.
    let mut state = vec![0u8; moves.len()];
    let mut steps = Vec::with_capacity(moves.len());
    let blocker = |index: usize, vacated: &[bool], state: &[u8]| {
        sources
            .get(&lower(&moves[index].to))
            .copied()
            .filter(|&other| other != index && !vacated[other] && state[other] != 2)
    };
    for start in 0..moves.len() {
        let mut stack = vec![start];
        while let Some(&index) = stack.last() {
            if state[index] == 2 {
                stack.pop();
                continue;
            }
            state[index] = 1;
            match blocker(index, &vacated, &state) {
                Some(other) if state[other] == 0 => {
                    stack.push(other);
                    continue;
                }
                Some(other) => {
                    let parked = temp(other);
                    steps.push(Step {
                        index: other,
                        from: current[other].clone(),
                        to: parked.clone(),
                    });
                    current[other] = parked;
                    vacated[other] = true;
                }
                None => {}
            }
            steps.push(Step {
                index,
                from: current[index].clone(),
                to: moves[index].to.clone(),
            });
            vacated[index] = true;
            state[index] = 2;
            stack.pop();
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capture(second: u8, millis: Option<u16>) -> Capture {
        Capture {
            time: WallClock::new(2026, 5, 1, 14, 3, second)
                .unwrap()
                .with_millis(millis),
            offset: None,
            source: TimeSource::ExifOriginal,
        }
    }
    fn member(path: &str, capture: Option<Capture>) -> Member {
        let path = PathBuf::from(path);
        let (stem, suffix, kind) = split_name(path.file_name().unwrap().to_str().unwrap()).unwrap();
        Member {
            path,
            kind,
            stem,
            suffix,
            size: 1,
            modified: 0,
            modified_local: WallClock::new(2020, 1, 2, 3, 4, 5),
            companion: false,
            probe: if kind.is_media() {
                Probe::Read(Metadata {
                    capture,
                    ..Metadata::default()
                })
            } else {
                Probe::Read(Metadata::default())
            },
        }
    }
    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .flat_map(|row| row.targets.iter())
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }
    fn run(groups: &[Group], disk: &[&str], options: Options) -> Vec<Row> {
        let disk: HashSet<String> = disk.iter().map(|p| p.to_lowercase()).collect();
        plan(groups, &vec![true; groups.len()], &options, |path| {
            disk.contains(&lower(path))
        })
    }
    fn options(template: &Template) -> Options<'_> {
        Options {
            template,
            mtime_fallback: false,
            lowercase_ext: false,
        }
    }
    #[test]
    fn names_split_with_double_sidecar_extensions() {
        assert_eq!(
            split_name("IMG_1.CR3.xmp"),
            Some(("IMG_1".into(), ".CR3.xmp".into(), Kind::Sidecar))
        );
        assert_eq!(
            split_name("IMG_1.xmp"),
            Some(("IMG_1".into(), ".xmp".into(), Kind::Sidecar))
        );
        assert_eq!(
            split_name("a.b.JPG"),
            Some(("a.b".into(), ".JPG".into(), Kind::Jpeg))
        );
        assert_eq!(split_name(".jpg"), None);
        assert_eq!(split_name("notes.txt"), None);
    }
    #[test]
    fn groups_pair_by_folder_and_stem() {
        let groups = group(vec![
            member(r"C:\a\IMG_1.HEIC", Some(capture(1, None))),
            member(r"C:\a\img_1.mov", None),
            member(r"C:\a\IMG_1.aae", None),
            member(r"C:\b\IMG_1.HEIC", Some(capture(2, None))),
            member(r"C:\a\lonely.xmp", None),
            member(r"C:\a\IMG_1.HEIC", Some(capture(1, None))),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].members.len(), 3);
        assert_eq!(groups[0].primary(), 0);
    }
    #[test]
    fn primary_prefers_still_images_with_time() {
        let raw_only = group(vec![
            member(r"C:\a\P.jpg", None),
            member(r"C:\a\P.cr3", Some(capture(1, None))),
        ]);
        assert_eq!(raw_only[0].primary(), 1);
        let both = group(vec![
            member(r"C:\a\P.cr3", Some(capture(1, None))),
            member(r"C:\a\P.jpg", Some(capture(1, None))),
            member(r"C:\a\P.mov", Some(capture(1, None))),
        ]);
        assert_eq!(both[0].primary(), 1);
    }
    #[test]
    fn group_members_share_the_new_stem() {
        let groups = group(vec![
            member(r"C:\a\IMG_1.HEIC", Some(capture(22, None))),
            member(r"C:\a\IMG_1.MOV", None),
            member(r"C:\a\IMG_1.HEIC.xmp", None),
        ]);
        let template = Template::parse(template::DEFAULT).unwrap();
        let mut opts = options(&template);
        opts.lowercase_ext = true;
        let rows = run(&groups, &[], opts);
        assert_eq!(
            names(&rows),
            [
                "2026-05-01_14-03-22.heic",
                "2026-05-01_14-03-22.mov",
                "2026-05-01_14-03-22.heic.xmp"
            ]
        );
        assert_eq!(rows[0].state, State::Ready);
        assert_eq!(rows[0].new_stem(&groups), Some("2026-05-01_14-03-22"));
    }
    #[test]
    fn collisions_use_subseconds_then_counters_in_order() {
        let groups = group(vec![
            member(r"C:\a\c.jpg", Some(capture(22, Some(900)))),
            member(r"C:\a\b.jpg", Some(capture(22, Some(457)))),
            member(r"C:\a\a.jpg", Some(capture(22, None))),
            member(r"C:\a\d.jpg", Some(capture(22, Some(457)))),
        ]);
        let template = Template::parse(template::DEFAULT).unwrap();
        let rows = run(
            &groups,
            &[r"C:\a\2026-05-01_14-03-22.jpg"],
            options(&template),
        );
        assert_eq!(
            names(&rows),
            [
                "2026-05-01_14-03-22_1.jpg",
                "2026-05-01_14-03-22_457.jpg",
                "2026-05-01_14-03-22_457_1.jpg",
                "2026-05-01_14-03-22_900.jpg"
            ]
        );
        assert!(rows.iter().all(|row| row.collided));
        let fff = Template::parse("{yyyy}{fff}").unwrap();
        let rows = run(&groups[..2], &[], options(&fff));
        assert_eq!(names(&rows), ["2026457.jpg", "2026900.jpg"]);
    }
    #[test]
    fn unchanged_and_case_only_renames() {
        let groups = group(vec![
            member(r"C:\a\2026-05-01_14-03-22.jpg", Some(capture(22, None))),
            member(r"C:\a\2026-05-01_14-03-23.JPG", Some(capture(23, None))),
        ]);
        let template = Template::parse(template::DEFAULT).unwrap();
        let mut opts = options(&template);
        opts.lowercase_ext = true;
        let disk = [
            r"C:\a\2026-05-01_14-03-22.jpg",
            r"C:\a\2026-05-01_14-03-23.JPG",
        ];
        let rows = run(&groups, &disk, opts);
        assert_eq!(rows[0].state, State::Unchanged);
        assert_eq!(rows[1].state, State::Ready);
        assert!(!rows[1].collided);
        assert_eq!(names(&rows)[1], "2026-05-01_14-03-23.jpg");
    }
    #[test]
    fn missing_dates_are_flagged_or_use_modified_time() {
        let groups = group(vec![member(r"C:\a\x.png", None)]);
        let template = Template::parse(template::DEFAULT).unwrap();
        let rows = run(&groups, &[], options(&template));
        assert_eq!(rows[0].state, State::NoDate);
        assert!(rows[0].targets.is_empty());
        let mut opts = options(&template);
        opts.mtime_fallback = true;
        let rows = run(&groups, &[], opts);
        assert_eq!(names(&rows), ["2020-01-02_03-04-05.png"]);
        assert_eq!(rows[0].capture.unwrap().source, TimeSource::Modified);
        let counter = Template::parse("{original}_{counter}").unwrap();
        let rows = run(&groups, &[], options(&counter));
        assert_eq!(names(&rows), ["x_001.png"]);
    }
    #[test]
    fn failed_probes_keep_their_state() {
        let mut online = member(r"C:\a\o.jpg", None);
        online.probe = Probe::OnlineOnly;
        let mut codec = member(r"C:\a\h.heic", None);
        codec.probe = Probe::CodecMissing("HEIF Image Extensions");
        let mut pending = member(r"C:\a\p.jpg", None);
        pending.probe = Probe::Pending;
        let groups = group(vec![online, codec, pending]);
        let template = Template::parse(template::DEFAULT).unwrap();
        let rows = run(&groups, &[], options(&template));
        let states: Vec<State> = rows.iter().map(|r| r.state).collect();
        assert!(states.contains(&State::OnlineOnly));
        assert!(states.contains(&State::CodecMissing("HEIF Image Extensions")));
        assert!(states.contains(&State::Pending));
        let summary = summary(&groups, &rows);
        assert_eq!((summary.failed, summary.pending, summary.ready), (2, 1, 0));
    }
    #[test]
    fn freed_names_are_reused_but_unselected_rows_stay_occupied() {
        let groups = group(vec![
            member(r"C:\a\2026-05-01_14-03-22.jpg", Some(capture(23, None))),
            member(r"C:\a\z.jpg", Some(capture(22, None))),
        ]);
        let template = Template::parse(template::DEFAULT).unwrap();
        let disk = [r"C:\a\2026-05-01_14-03-22.jpg", r"C:\a\z.jpg"];
        let rows = run(&groups, &disk, options(&template));
        assert_eq!(
            names(&rows),
            ["2026-05-01_14-03-22.jpg", "2026-05-01_14-03-23.jpg"]
        );
        assert!(rows.iter().all(|row| !row.collided));
        let disk_set: HashSet<String> = disk.iter().map(|p| p.to_lowercase()).collect();
        let rows = plan(&groups, &[false, true], &options(&template), |path| {
            disk_set.contains(&lower(path))
        });
        let z = rows.iter().find(|row| row.group == 1).unwrap();
        assert!(z.collided);
    }
    #[test]
    fn exhausted_counters_fail_the_row() {
        let groups = group(vec![member(r"C:\a\x.jpg", Some(capture(22, None)))]);
        let template = Template::parse("same").unwrap();
        let rows = plan(&groups, &[true], &options(&template), |_| true);
        assert_eq!(rows[0].state, State::Conflict);
    }
    #[test]
    fn chains_run_in_dependency_order_and_swaps_use_a_temporary_name() {
        let mv = |from: &str, to: &str| Move {
            from: from.into(),
            to: to.into(),
        };
        let chain = [mv("A", "B"), mv("B", "C")];
        let steps = order(&chain, |_| unreachable!());
        let pairs: Vec<(&str, &str)> = steps
            .iter()
            .map(|s| (s.from.to_str().unwrap(), s.to.to_str().unwrap()))
            .collect();
        assert_eq!(pairs, [("B", "C"), ("A", "B")]);
        let swap = [mv("A", "B"), mv("B", "A")];
        let steps = order(&swap, |index| PathBuf::from(format!("T{index}")));
        let pairs: Vec<(&str, &str)> = steps
            .iter()
            .map(|s| (s.from.to_str().unwrap(), s.to.to_str().unwrap()))
            .collect();
        assert_eq!(pairs, [("A", "T0"), ("B", "A"), ("T0", "B")]);
        let case_only = [mv("a.jpg", "A.jpg")];
        assert_eq!(order(&case_only, |_| unreachable!()).len(), 1);
        let long: Vec<Move> = (0..20_000)
            .map(|n| mv(&format!("f{n}"), &format!("f{}", n + 1)))
            .collect();
        let steps = order(&long, |_| unreachable!());
        assert_eq!(steps.first().unwrap().index, 19_999);
        assert_eq!(steps.len(), 20_000);
    }
}

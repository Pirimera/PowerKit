//! Installed and Discover list modes, filters, sort keys and the query that builds the visible rows.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListMode {
    #[default]
    Installed,
    Discover,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusFilter {
    #[default]
    All,
    UpdatesOnly,
    UpToDate,
    Pinned,
}

impl StatusFilter {
    pub const ALL: [Self; 4] = [Self::All, Self::UpdatesOnly, Self::UpToDate, Self::Pinned];

    pub fn index(self) -> usize {
        match self {
            Self::All => 0,
            Self::UpdatesOnly => 1,
            Self::UpToDate => 2,
            Self::Pinned => 3,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScopeFilter {
    #[default]
    All,
    User,
    Machine,
}

impl ScopeFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::User, Self::Machine];

    pub fn index(self) -> usize {
        match self {
            Self::All => 0,
            Self::User => 1,
            Self::Machine => 2,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    Name,
    Id,
    InstalledDate,
    UpdatedDate,
}

impl SortKey {
    pub const ALL: [Self; 4] = [Self::Name, Self::Id, Self::InstalledDate, Self::UpdatedDate];

    pub fn index(self) -> usize {
        match self {
            Self::Name => 0,
            Self::Id => 1,
            Self::InstalledDate => 2,
            Self::UpdatedDate => 3,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn default_ascending(self) -> bool {
        matches!(self, Self::Name | Self::Id)
    }
}

/// One entry of the combined sort picker: every key in both directions, ascending first.
pub fn sort_choice_index(key: SortKey, ascending: bool) -> usize {
    key.index() * 2 + usize::from(!ascending)
}

pub fn sort_choice_from_index(index: usize) -> Option<(SortKey, bool)> {
    SortKey::from_index(index / 2).map(|key| (key, index.is_multiple_of(2)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListViewQuery {
    pub filter_text: String,
    /// Lowercase source names such as `"winget"`; empty means all sources.
    pub sources: BTreeSet<String>,
    pub status: StatusFilter,
    pub scope: ScopeFilter,
    pub sort_by: SortKey,
    pub sort_ascending: bool,
}

impl Default for ListViewQuery {
    fn default() -> Self {
        Self {
            filter_text: String::new(),
            sources: BTreeSet::new(),
            status: StatusFilter::All,
            scope: ScopeFilter::All,
            sort_by: SortKey::Name,
            sort_ascending: true,
        }
    }
}

pub fn filter_updatable<'a>(
    packages: impl IntoIterator<Item = &'a PackageSummary>,
    filters: &UpdateFilters,
) -> Vec<&'a PackageSummary> {
    packages
        .into_iter()
        .filter(|package| package.has_actionable_update(filters))
        .collect()
}

/// Distinct source names in display order (known managers first, then extras).
pub fn available_sources(packages: &[PackageSummary]) -> Vec<String> {
    const KNOWN: &[&str] = &[
        "winget",
        "msstore",
        "chocolatey",
        "scoop",
        "npm",
        "pip",
        "cargo",
        "dotnet",
        "powershell",
        "powershell7",
        "vcpkg",
    ];
    let mut seen = BTreeSet::new();
    for package in packages {
        let source = package.key.source.trim().to_ascii_lowercase();
        if !source.is_empty() {
            seen.insert(source);
        }
    }
    let mut sources = Vec::new();
    for known in KNOWN {
        if *known == "winget" || *known == "msstore" || seen.contains(*known) {
            sources.push((*known).to_string());
            seen.remove(*known);
        }
    }
    sources.extend(seen);
    sources
}

/// Filters and sorts a package list. In `ListMode::Installed` with `StatusFilter::All`,
/// actionable updates always stay pinned above the rest of the installed packages.
pub fn query_packages<'a>(
    packages: &'a [PackageSummary],
    mode: ListMode,
    query: &ListViewQuery,
    prefs: &PowerStorePreferences,
) -> Vec<&'a PackageSummary> {
    let needle = query.filter_text.trim().to_lowercase();
    let mut visible: Vec<&'a PackageSummary> = packages
        .iter()
        .filter(|package| {
            if !needle.is_empty() {
                let matches_text = package.name.to_lowercase().contains(&needle)
                    || package.key.id.to_lowercase().contains(&needle)
                    || package.key.source.to_lowercase().contains(&needle);
                if !matches_text {
                    return false;
                }
            }
            if !query.sources.is_empty()
                && !query
                    .sources
                    .contains(&package.key.source.to_ascii_lowercase())
            {
                return false;
            }
            if mode == ListMode::Installed {
                let matches_status = match query.status {
                    StatusFilter::All => true,
                    StatusFilter::UpdatesOnly => package.has_actionable_update(&prefs.filters),
                    StatusFilter::UpToDate => !package.has_actionable_update(&prefs.filters),
                    StatusFilter::Pinned => package.is_pinned(&prefs.filters),
                };
                if !matches_status {
                    return false;
                }
                let effective_scope = package.scope.as_deref().or_else(|| {
                    prefs
                        .options
                        .get(&package.key)
                        .and_then(|options| options.scope.as_deref())
                });
                let matches_scope = match query.scope {
                    ScopeFilter::All => true,
                    ScopeFilter::User => {
                        effective_scope.is_some_and(|s| s.eq_ignore_ascii_case("user"))
                    }
                    ScopeFilter::Machine => {
                        effective_scope.is_some_and(|s| s.eq_ignore_ascii_case("machine"))
                    }
                };
                if !matches_scope {
                    return false;
                }
            }
            true
        })
        .collect();

    let pin_updates = mode == ListMode::Installed && query.status == StatusFilter::All;
    visible.sort_by(|left, right| {
        if pin_updates {
            let left_group = u8::from(!left.has_actionable_update(&prefs.filters));
            let right_group = u8::from(!right.has_actionable_update(&prefs.filters));
            let group_cmp = left_group.cmp(&right_group);
            if group_cmp != std::cmp::Ordering::Equal {
                return group_cmp;
            }
        }
        compare_packages(left, right, query.sort_by, query.sort_ascending)
    });
    visible
}

fn compare_packages(
    left: &PackageSummary,
    right: &PackageSummary,
    sort_by: SortKey,
    ascending: bool,
) -> std::cmp::Ordering {
    let tie_break = || {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.key.id.to_lowercase().cmp(&right.key.id.to_lowercase()))
    };
    match sort_by {
        SortKey::Name => {
            let ordering = tie_break();
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        }
        SortKey::Id => {
            let ordering = left
                .key
                .id
                .to_lowercase()
                .cmp(&right.key.id.to_lowercase())
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        }
        SortKey::InstalledDate | SortKey::UpdatedDate => {
            fn package_date(package: &PackageSummary, sort_by: SortKey) -> Option<&str> {
                let raw = match sort_by {
                    SortKey::InstalledDate => package.install_date.as_deref(),
                    SortKey::UpdatedDate => package.updated_at.as_deref(),
                    _ => None,
                };
                raw.filter(|value| !value.is_empty())
            }
            match (package_date(left, sort_by), package_date(right, sort_by)) {
                (Some(left_date), Some(right_date)) => {
                    let ordering = left_date.cmp(right_date);
                    let directed = if ascending {
                        ordering
                    } else {
                        ordering.reverse()
                    };
                    directed.then_with(tie_break)
                }
                // Packages with known dates always appear before unknown dates.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => tie_break(),
            }
        }
    }
}

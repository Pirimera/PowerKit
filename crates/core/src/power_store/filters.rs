//! Ignored and skipped updates; they keep packages out of the update count.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateFilters {
    ignored: BTreeSet<PackageKey>,
    skipped: BTreeMap<PackageKey, String>,
}

impl UpdateFilters {
    pub fn ignore(&mut self, key: PackageKey) -> bool {
        if self.ignored.len() >= MAX_IGNORE_ENTRIES && !self.ignored.contains(&key) {
            return false;
        }
        self.skipped.remove(&key);
        self.ignored.insert(key);
        true
    }

    pub fn clear_ignore(&mut self, key: &PackageKey) {
        self.ignored.remove(key);
    }

    pub fn skip_version(&mut self, key: PackageKey, version: impl Into<String>) -> bool {
        if self.ignored.contains(&key) {
            return true;
        }
        if self.skipped.len() >= MAX_SKIP_ENTRIES && !self.skipped.contains_key(&key) {
            return false;
        }
        self.skipped.insert(key, version.into());
        true
    }

    pub fn clear_skip(&mut self, key: &PackageKey) {
        self.skipped.remove(key);
    }

    pub fn blocks_update(&self, key: &PackageKey, available_version: &str) -> bool {
        if key.is_own_package() || self.ignored.contains(key) {
            return true;
        }
        self.skipped
            .get(key)
            .is_some_and(|skipped| skipped == available_version)
    }

    pub fn is_ignored(&self, key: &PackageKey) -> bool {
        self.ignored.contains(key)
    }

    pub fn skipped_version(&self, key: &PackageKey) -> Option<&str> {
        self.skipped.get(key).map(String::as_str)
    }

    pub fn ignored(&self) -> impl Iterator<Item = &PackageKey> {
        self.ignored.iter()
    }

    pub fn skipped(&self) -> impl Iterator<Item = (&PackageKey, &String)> {
        self.skipped.iter()
    }
}

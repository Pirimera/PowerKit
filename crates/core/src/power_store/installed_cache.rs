//! The last Installed list, saved so a cold start can show rows before any manager answers.

use super::*;

const HEADER: &str = "powerkit-installed=1";
const CHECKED: &str = "checked";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstalledCache {
    /// Local wall-clock text of the list load that produced these rows.
    pub checked_at: String,
    pub packages: Vec<PackageSummary>,
    pub store_managed: BTreeSet<PackageKey>,
}

impl InstalledCache {
    pub fn serialize(&self) -> String {
        let mut text = format!("{HEADER}\n{CHECKED}\t{}\n", escape_field(&self.checked_at));
        for package in &self.packages {
            let Some(key) = encode_package_key(&package.key) else {
                continue;
            };
            let optional = |value: &Option<String>| value.clone().unwrap_or_default();
            let fields = [
                key,
                package.name.clone(),
                optional(&package.version),
                optional(&package.available_version),
                flag(package.update_available).into(),
                optional(&package.scope),
                optional(&package.install_date),
                optional(&package.updated_at),
                optional(&package.icon),
                flag(self.store_managed.contains(&package.key)).into(),
            ];
            let line: Vec<String> = fields.iter().map(|field| escape_field(field)).collect();
            text.push_str(&line.join("\t"));
            text.push('\n');
        }
        text
    }

    /// `None` for a missing or foreign header; malformed rows are skipped.
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return None;
        }
        let mut cache = Self::default();
        for line in lines {
            let fields: Vec<String> = line.split('\t').map(unescape_field).collect();
            match fields.as_slice() {
                [name, checked_at] if name == CHECKED => cache.checked_at = checked_at.clone(),
                [
                    key,
                    name,
                    version,
                    available,
                    update,
                    scope,
                    install_date,
                    updated_at,
                    icon,
                    store_managed,
                ] => {
                    let Some(key) = parse_package_key(key) else {
                        continue;
                    };
                    if store_managed == "1" {
                        cache.store_managed.insert(key.clone());
                    }
                    let optional = |value: &String| (!value.is_empty()).then(|| value.clone());
                    cache.packages.push(PackageSummary {
                        key,
                        name: name.clone(),
                        version: optional(version),
                        available_version: optional(available),
                        update_available: update == "1",
                        scope: optional(scope),
                        install_date: optional(install_date),
                        updated_at: optional(updated_at),
                        icon: optional(icon),
                    });
                }
                _ => {}
            }
        }
        Some(cache)
    }
}

fn flag(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

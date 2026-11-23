//! Logo loader: finds local and database logos for rows that lack one, off the UI thread.

use super::*;

/// Progress of the background logo loader that follows every list load.
pub(super) enum IconMessage {
    /// Icon URLs found in the remote icon database for rows that had none.
    Assign(Vec<(PackageKey, String)>),
    /// One more logo finished decoding; the snapshot should be rebuilt.
    Ready,
}

/// A row needs a logo when it has none, or its local `DisplayIcon` produced nothing.
fn needs_icon(catalog: &dyn PackageCatalog, package: &PackageSummary) -> bool {
    match package.icon.as_deref() {
        None | Some("") => true,
        Some(location) => !is_http_url(location) && catalog.cached_icon(location).is_none(),
    }
}

/// Keeps logos found earlier (database URLs) across a reload so rows do not flash back
/// to initials while the loader runs again.
pub(super) fn carry_icons(
    catalog: &dyn PackageCatalog,
    previous: &[PackageSummary],
    fresh: &mut [PackageSummary],
) {
    let known: BTreeMap<&PackageKey, &String> = previous
        .iter()
        .filter_map(|package| package.icon.as_ref().map(|icon| (&package.key, icon)))
        .collect();
    for package in fresh {
        if needs_icon(catalog, package)
            && let Some(icon) = known.get(&package.key)
        {
            package.icon = Some((*icon).clone());
        }
    }
}

impl Session {
    /// Starts the logo loader for rows that still lack a decoded logo: database lookup
    /// (when enabled), then parallel downloads. Never blocks the UI thread.
    pub(super) fn start_icon_load(&mut self) {
        let wanted: Vec<PackageSummary> = self
            .installed_packages
            .iter()
            .chain(&self.discover_packages)
            .filter(|package| needs_icon(self.catalog.as_ref(), package))
            .cloned()
            .collect();
        let downloads: Vec<String> = self
            .installed_packages
            .iter()
            .chain(&self.discover_packages)
            .filter_map(|package| package.icon.clone())
            .filter(|location| is_http_url(location))
            .collect();
        if wanted.is_empty() && downloads.is_empty() {
            return;
        }
        let remote_icons = self.remote_icons;
        let (sender, receiver) = mpsc::channel();
        self.icon_receiver = Some(receiver);
        let catalog = self.catalog.clone();
        std::thread::spawn(move || {
            let mut locations = downloads;
            if remote_icons {
                let assigned = catalog.resolve_icons(&wanted);
                locations.extend(assigned.iter().map(|(_, url)| url.clone()));
                if !assigned.is_empty() {
                    let _ = sender.send(IconMessage::Assign(assigned));
                }
            }
            catalog.fetch_icons(&locations, &|| {
                let _ = sender.send(IconMessage::Ready);
            });
        });
    }

    pub(super) fn poll_icons(&mut self) -> bool {
        let Some(receiver) = &self.icon_receiver else {
            return false;
        };
        let mut messages = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(message) => messages.push(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.icon_receiver = None;
                    break;
                }
            }
        }
        let changed = !messages.is_empty();
        for message in messages {
            if let IconMessage::Assign(updates) = message {
                self.assign_icons(&updates);
            }
        }
        changed
    }

    fn assign_icons(&mut self, updates: &[(PackageKey, String)]) {
        let by_key: BTreeMap<&PackageKey, &String> =
            updates.iter().map(|(key, url)| (key, url)).collect();
        let catalog = self.catalog.clone();
        let rows = self
            .installed_packages
            .iter_mut()
            .chain(self.discover_packages.iter_mut())
            .chain(self.updates.iter_mut().flatten());
        for package in rows {
            if needs_icon(catalog.as_ref(), package)
                && let Some(url) = by_key.get(&package.key)
            {
                package.icon = Some((*url).clone());
            }
        }
    }
}

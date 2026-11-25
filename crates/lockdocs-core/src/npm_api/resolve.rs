//! Follow declaration graph edges for symbol and documentation lookup.

use super::{module::Binding, PublicApi, Resolution, Root};
use crate::extract::{Entry, Kind};
use std::collections::{BTreeMap, BTreeSet};

type ProseSurface<'a> = (&'a str, BTreeSet<&'a str>);

impl PublicApi {
    pub fn resolve(&self, root: &Root, symbol: &str, entries: &[Entry]) -> Resolution {
        let mut out = Resolution::default();
        self.exported(&root.file, symbol, entries, &mut BTreeSet::new(), &mut out);
        out
    }

    /// Search the declaration graph of this import, including referenced types
    /// and members, without admitting other editions shipped in the tarball.
    pub(crate) fn search_files(&self, import: &str) -> BTreeSet<String> {
        if self.legacy_subpaths && self.roots.first().is_some_and(|root| root.import == import) {
            return self.modules.keys().cloned().collect();
        }
        let mut files = BTreeSet::new();
        let mut pending: Vec<&str> = self.roots.iter().filter(|r| r.import == import).map(|r| r.file.as_str()).collect();
        while let Some(file) = pending.pop() {
            if !files.insert(file.to_string()) {
                continue;
            }
            if let Some(m) = self.modules.get(file) {
                pending.extend(m.stars.iter().map(String::as_str));
                for b in m.exports.values().chain(m.imports.values()) {
                    if let Binding::Named(target, _) | Binding::Namespace(target) = b {
                        pending.push(target);
                    }
                }
            }
        }
        files
    }

    /// Identical entrypoints and transparent full-surface barrels share prose.
    /// Root searches also cover additive public helpers with disjoint names.
    /// Shared dependencies and legacy search widening never establish edition
    /// compatibility.
    pub(crate) fn compatible_imports(&self, selected: &str, package: &str) -> BTreeSet<&str> {
        let mut targets: BTreeMap<&str, Option<BTreeSet<ProseSurface<'_>>>> = BTreeMap::new();
        for root in &self.roots {
            let files = targets.entry(&root.import).or_insert_with(|| Some(BTreeSet::new()));
            match (files.as_mut(), self.prose_surface(&root.file)) {
                (Some(files), Some(file)) => {
                    files.insert(file);
                }
                _ => *files = None,
            }
        }
        let Some(Some(selected_files)) = targets.get(selected) else {
            return BTreeSet::new();
        };
        // A root package search also covers additive public helpers. Their
        // names must be fully known and disjoint from the root surface; graph
        // intersection never establishes compatibility between API editions.
        let public_names = |import: &str| -> Option<BTreeSet<String>> {
            let mut names = BTreeSet::new();
            for root in self.roots.iter().filter(|r| r.import == import) {
                names.extend(self.export_names(&root.file, &mut BTreeSet::new())?);
            }
            (!names.is_empty()).then_some(names)
        };
        let selected_names = (selected == package).then(|| public_names(selected)).flatten();
        targets
            .iter()
            .filter_map(|(&import, files)| {
                (import == selected
                    || selected_files.len() == 1 && files.as_ref() == Some(selected_files)
                    || selected_names
                        .as_ref()
                        .is_some_and(|names| public_names(import).is_some_and(|other| names.is_disjoint(&other))))
                .then_some(import)
            })
            .collect()
    }

    fn export_names(&self, file: &str, seen: &mut BTreeSet<String>) -> Option<BTreeSet<String>> {
        if seen.len() > 128 || !seen.insert(file.to_string()) {
            return None;
        }
        let m = self.modules.get(file)?;
        if m.incomplete || m.assignment.is_some() {
            return None;
        }
        let mut names: BTreeSet<String> = m.exports.keys().cloned().collect();
        for target in &m.stars {
            let mut forwarded = self.export_names(target, seen)?;
            forwarded.remove("default");
            names.extend(forwarded);
        }
        seen.remove(file);
        Some(names)
    }

    // A barrel can also expose the same module as named/default namespaces.
    // Preserve the alias names in the identity: merely sharing a star target
    // does not make wrappers with different public bindings interchangeable.
    fn prose_surface<'a>(&'a self, file: &'a str) -> Option<ProseSurface<'a>> {
        let canonical = self.canonical_barrel(file)?;
        let module = self.modules.get(canonical)?;
        if !module.incomplete && module.assignment.is_none() && module.stars.len() == 1 {
            let target = module.stars[0].as_str();
            let mut aliases = BTreeSet::new();
            for (name, binding) in &module.exports {
                let binding = match binding {
                    Binding::Local(local) => module.imports.get(local).unwrap_or(binding),
                    other => other,
                };
                match binding {
                    Binding::Namespace(source) if source == target => {
                        aliases.insert(name.as_str());
                    }
                    _ => return Some((canonical, BTreeSet::new())),
                }
            }
            if !aliases.is_empty() {
                return Some((self.canonical_barrel(target)?, aliases));
            }
        }
        Some((canonical, BTreeSet::new()))
    }

    fn canonical_barrel<'a>(&'a self, mut file: &'a str) -> Option<&'a str> {
        let mut seen = BTreeSet::new();
        while seen.insert(file) {
            let m = self.modules.get(file)?;
            if !m.incomplete && m.assignment.is_none() && m.stars.len() == 1 {
                let target = m.stars[0].as_str();
                let forwarded = |(export, b): (&String, &Binding)| match b {
                    Binding::Named(f, name) => f == target && name == export && name != "default",
                    Binding::Local(name) => {
                        matches!(m.imports.get(name), Some(Binding::Named(f, original)) if f == target && original == export && original != "default")
                    }
                    Binding::Namespace(_) => false,
                };
                if m.exports.iter().all(forwarded) && self.modules.contains_key(target) {
                    file = target;
                    continue;
                }
            }
            return Some(file);
        }
        None
    }

    pub(crate) fn search_alias<'a>(&'a self, entry: &'a Entry) -> &'a Entry {
        if entry.alias_of.is_none() {
            return entry;
        }
        let mut found = Resolution::default();
        self.exported(&entry.file, &entry.path, &self.entries, &mut BTreeSet::new(), &mut found);
        if !found.unresolved && found.entries.len() == 1 {
            &self.entries[*found.entries.first().unwrap()]
        } else {
            entry
        }
    }

    fn exported(&self, file: &str, path: &str, entries: &[Entry], seen: &mut BTreeSet<(String, String)>, out: &mut Resolution) {
        if seen.len() > 128 {
            out.unresolved = true;
            return;
        }
        let key = (file.to_string(), path.to_string());
        if !seen.insert(key.clone()) {
            return;
        }
        let Some(m) = self.modules.get(file) else {
            out.unresolved = true;
            seen.remove(&key);
            return;
        };
        out.unresolved |= m.incomplete;
        let (name, tail) = path.split_once('.').unwrap_or((path, ""));
        if let Some(b) = m.exports.get(name) {
            self.binding(file, b, tail, entries, seen, out);
        } else if let Some(namespace) = &m.assignment {
            let local = if path.starts_with(&format!("{namespace}.")) {
                path.to_string()
            } else {
                format!("{namespace}.{path}")
            };
            self.binding(file, &Binding::Local(local), "", entries, seen, out);
        } else if name != "default" {
            for star in &m.stars {
                self.exported(star, path, entries, seen, out);
            }
        }
        seen.remove(&key);
    }

    fn binding(&self, file: &str, b: &Binding, tail: &str, entries: &[Entry], seen: &mut BTreeSet<(String, String)>, out: &mut Resolution) {
        let extend = |s: &str| if tail.is_empty() { s.to_string() } else { format!("{s}.{tail}") };
        match b {
            Binding::Namespace(target) => {
                if tail.is_empty() {
                    out.unresolved = true;
                } else {
                    self.exported(target, tail, entries, seen, out);
                }
            }
            Binding::Named(target, name) => self.exported(target, &extend(name), entries, seen, out),
            Binding::Local(name) => {
                if let Some(import) = self.modules.get(file).and_then(|m| m.imports.get(name)) {
                    self.binding(file, import, tail, entries, seen, out);
                } else {
                    let path = extend(name);
                    let mut found = false;
                    for (i, e) in entries.iter().enumerate() {
                        if e.file == file && e.path == path && e.kind != Kind::Alias && e.kind != Kind::Prose {
                            out.entries.insert(i);
                            found = true;
                        }
                    }
                    if !found {
                        out.unresolved = true;
                    }
                }
            }
        }
    }
}

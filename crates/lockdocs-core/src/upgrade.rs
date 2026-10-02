//! Upgrade report (lockdocs Pro): the API difference between the pinned
//! version of a package and a target version, limited to the symbols the
//! project actually uses, with call sites and migration-guide pointers.
//!
//! This module only compares and scans; whether the caller may run it is
//! decided by the binary's licence gate (`crates/lockdocs/src/pro.rs`).

use crate::extract::{Entry, Kind};
use crate::index::PackageIndex;
use crate::{norm_name, Eco};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// What happened to a symbol between the two versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Removed,
    /// Renamed to the `new` entry (matched by signature shape or doc similarity).
    Renamed,
    SignatureChanged,
    Deprecated,
}

impl Change {
    pub fn as_str(self) -> &'static str {
        match self {
            Change::Removed => "removed",
            Change::Renamed => "renamed",
            Change::SignatureChanged => "signature_changed",
            Change::Deprecated => "deprecated",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diff {
    pub change: Change,
    pub old: Entry,
    pub new: Option<Entry>,
}

/// A place in the project that mentions a changed symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    pub path: String,
    pub line: u32,
}

fn is_api(e: &Entry) -> bool {
    e.kind != Kind::Prose && !e.legacy
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parent(path: &str) -> &str {
    path.rfind(['.', ':']).map_or("", |i| path[..i].trim_end_matches(':'))
}

/// The signature with the symbol's own name blanked, to compare across renames.
fn shape(e: &Entry) -> String {
    squash(&e.sig.replacen(&e.name, "_", 1))
}

fn deprecated(e: &Entry) -> bool {
    let doc = e.doc.to_ascii_lowercase();
    let sig = e.sig.to_ascii_lowercase();
    doc.contains("@deprecated") || doc.contains("deprecated:") || doc.contains(".. deprecated::") || sig.contains("#[deprecated") || sig.contains("@deprecated")
}

fn words(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn similarity(a: &str, b: &str) -> f32 {
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(&b).count() as f32 / a.union(&b).count() as f32
}

/// Symbol-level difference. A path present in the new version (as a symbol or
/// as a re-export) is never "removed".
pub fn diff(old: &[Entry], new: &[Entry]) -> Vec<Diff> {
    let new_by_path: HashMap<&str, &Entry> = new.iter().filter(|e| is_api(e)).map(|e| (e.path.as_str(), e)).collect();
    let old_paths: HashSet<&str> = old.iter().filter(|e| is_api(e)).map(|e| e.path.as_str()).collect();
    let mut added: Vec<&Entry> = new
        .iter()
        .filter(|e| is_api(e) && e.alias_of.is_none() && !old_paths.contains(e.path.as_str()))
        .collect();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for o in old.iter().filter(|e| is_api(e)) {
        if !seen.insert(o.path.as_str()) {
            continue;
        }
        match new_by_path.get(o.path.as_str()) {
            Some(n) => {
                if deprecated(n) && !deprecated(o) {
                    out.push(Diff {
                        change: Change::Deprecated,
                        old: o.clone(),
                        new: Some((*n).clone()),
                    });
                } else if o.alias_of.is_none() && n.alias_of.is_none() && squash(&o.sig) != squash(&n.sig) {
                    out.push(Diff {
                        change: Change::SignatureChanged,
                        old: o.clone(),
                        new: Some((*n).clone()),
                    });
                }
            }
            None => {
                // Best rename candidate: same kind and parent, equal signature shape or similar docs.
                let best = added
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.kind == o.kind && parent(&a.path) == parent(&o.path))
                    .map(|(i, a)| {
                        let score = if !o.sig.is_empty() && shape(o) == shape(a) {
                            1.0
                        } else {
                            similarity(&o.doc, &a.doc)
                        };
                        (i, score)
                    })
                    .filter(|(_, s)| *s >= 0.7)
                    .max_by(|x, y| x.1.total_cmp(&y.1));
                match best {
                    Some((i, _)) => {
                        let n = added.remove(i);
                        out.push(Diff {
                            change: Change::Renamed,
                            old: o.clone(),
                            new: Some(n.clone()),
                        });
                    }
                    None => out.push(Diff {
                        change: Change::Removed,
                        old: o.clone(),
                        new: None,
                    }),
                }
            }
        }
    }
    out
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "vendor",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    "coverage",
    ".tox",
    "site-packages",
];
const MAX_FILES: usize = 5_000;
const MAX_FILE_BYTES: u64 = 1024 * 1024;

fn extensions(eco: Eco) -> &'static [&'static str] {
    match eco {
        Eco::Npm => &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "vue", "svelte"],
        Eco::PyPI => &["py"],
        Eco::Cargo => &["rs"],
        Eco::Go => &["go"],
    }
}

/// Strings that show a file uses the package.
fn markers(eco: Eco, name: &str) -> Vec<String> {
    let mut m = vec![name.to_string()];
    match eco {
        Eco::PyPI => {
            m.push(norm_name(eco, name).replace('-', "_"));
            m.push(name.replace('-', "_"));
        }
        Eco::Cargo => m.push(name.replace('-', "_")),
        _ => {}
    }
    m
}

fn walk(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if out.len() >= MAX_FILES {
            return;
        }
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        match e.file_type() {
            Ok(t) if t.is_dir() => {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    walk(&path, exts, out);
                }
            }
            Ok(t) if t.is_file() => {
                let ok_ext = path.extension().and_then(|x| x.to_str()).is_some_and(|x| exts.contains(&x));
                if ok_ext && e.metadata().is_ok_and(|m| m.len() <= MAX_FILE_BYTES) {
                    out.push(path);
                }
            }
            _ => {}
        }
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Does `line` use `name` as a whole identifier (members must follow `.` or `::`)?
fn mentions(line: &str, name: &str, member: bool) -> bool {
    let mut from = 0;
    while let Some(i) = line[from..].find(name) {
        let s = from + i;
        let e = s + name.len();
        let before = line[..s].chars().next_back();
        let after = line[e..].chars().next();
        let bounded = !before.is_some_and(is_ident) && !after.is_some_and(is_ident);
        let member_ok = !member || line[..s].ends_with('.') || line[..s].ends_with("::");
        if bounded && member_ok {
            return true;
        }
        from = e;
    }
    false
}

/// Call sites per old symbol path, found by a textual scan of the project's
/// source files that mention the package. Not a type-aware analysis: a member
/// name matches when it follows `.` or `::`.
pub fn call_sites(root: &Path, eco: Eco, package: &str, symbols: &[&Entry]) -> HashMap<String, Vec<CallSite>> {
    let mut files = Vec::new();
    walk(root, extensions(eco), &mut files);
    let marks = markers(eco, package);
    let mut found: HashMap<String, Vec<CallSite>> = HashMap::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        if !marks.iter().any(|m| text.contains(m.as_str())) {
            continue;
        }
        let rel = f.strip_prefix(root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        for (i, line) in text.lines().enumerate() {
            for s in symbols {
                if s.name.is_empty() || !mentions(line, &s.name, s.kind == Kind::Method || s.kind == Kind::Field) {
                    continue;
                }
                let v = found.entry(s.path.clone()).or_default();
                if v.len() < 20 {
                    v.push(CallSite {
                        path: rel.clone(),
                        line: i as u32 + 1,
                    });
                }
            }
        }
    }
    found
}

fn is_guide(file: &str) -> bool {
    let base = file.rsplit('/').next().unwrap_or(file).to_ascii_uppercase();
    ["CHANGELOG", "MIGRAT", "UPGRAD", "BREAKING", "HISTORY", "NEWS"]
        .iter()
        .any(|k| base.contains(k))
}

fn cite(idx: &PackageIndex, e: &Entry) -> String {
    format!("{}@{} {}:{}", idx.name, idx.version, e.file, e.line)
}

/// Migration-guide sections of the target version: those that mention the symbol.
fn guide_for<'a>(idx: &'a PackageIndex, name: &str) -> Vec<&'a Entry> {
    idx.entries
        .iter()
        .filter(|e| e.kind == Kind::Prose && is_guide(&e.file) && !name.is_empty() && mentions(&e.doc, name, false))
        .take(2)
        .collect()
}

fn side(idx: &PackageIndex, e: &Entry) -> Value {
    json!({"cite": cite(idx, e), "kind": e.kind.as_str(), "signature": squash(&e.sig)})
}

/// The report: the changes the project is exposed to, the rest as a count.
pub struct Report {
    pub text: String,
    pub json: Value,
}

pub fn report(root: &Path, old: &PackageIndex, new: &PackageIndex) -> Report {
    let diffs = diff(&old.entries, &new.entries);
    let olds: Vec<&Entry> = diffs.iter().map(|d| &d.old).collect();
    let sites = call_sites(root, old.eco, &old.name, &olds);
    let mut affected = Vec::new();
    let mut text = format!(
        "Upgrade report: {} {} -> {} ({} API changes between the versions)\n",
        old.name,
        old.version,
        new.version,
        diffs.len()
    );
    let mut other = 0;
    for d in &diffs {
        let Some(cs) = sites.get(&d.old.path).filter(|c| !c.is_empty()) else {
            other += 1;
            continue;
        };
        let guide = guide_for(new, &d.old.name);
        text.push_str(&format!(
            "\n{} {}\n  was: {}  [{}]\n",
            d.change.as_str().to_uppercase(),
            d.old.path,
            squash(&d.old.sig),
            cite(old, &d.old)
        ));
        if let Some(n) = &d.new {
            text.push_str(&format!("  now: {}  [{}]\n", squash(&n.sig), cite(new, n)));
        }
        text.push_str(&format!(
            "  call sites: {}\n",
            cs.iter().map(|c| format!("{}:{}", c.path, c.line)).collect::<Vec<_>>().join(", ")
        ));
        for g in &guide {
            text.push_str(&format!("  migration: {} [{}]\n", g.name, cite(new, g)));
        }
        affected.push(json!({
            "change": d.change.as_str(),
            "symbol": d.old.path,
            "old": side(old, &d.old),
            "new": d.new.as_ref().map(|n| side(new, n)),
            "call_sites": cs.iter().map(|c| json!({"path": c.path, "line": c.line})).collect::<Vec<_>>(),
            "migration": guide.iter().map(|g| json!({"section": g.name, "cite": cite(new, g)})).collect::<Vec<_>>(),
        }));
    }
    if affected.is_empty() {
        text.push_str("\nNo changed API is used in this project's source files.\n");
    }
    let guides: Vec<&Entry> = new.entries.iter().filter(|e| e.kind == Kind::Prose && is_guide(&e.file)).take(5).collect();
    if !guides.is_empty() {
        text.push_str("\nMigration guide sections in the target version:\n");
        for g in &guides {
            text.push_str(&format!("  {} [{}]\n", g.name, cite(new, g)));
        }
    }
    text.push_str(&format!(
        "\n{other} other API changes are not used in this project. Call sites are textual matches in files that mention the package.\n"
    ));
    let json = json!({
        "package": old.name, "from": old.version, "to": new.version,
        "api_changes": diffs.len(), "affected": affected, "unaffected_changes": other,
        "migration_guides": guides.iter().map(|g| json!({"section": g.name, "cite": cite(new, g)})).collect::<Vec<_>>(),
    });
    Report { text, json }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(kind: Kind, path: &str, sig: &str, doc: &str) -> Entry {
        Entry {
            kind,
            name: path.rsplit(['.', ':']).next().unwrap().to_string(),
            path: path.into(),
            file: "index.d.ts".into(),
            line: 1,
            sig: sig.into(),
            doc: doc.into(),
            alias_of: None,
            legacy: false,
        }
    }

    #[test]
    fn classifies_changes() {
        let old = vec![
            e(Kind::Function, "p.gone", "function gone(): void", "Does the thing."),
            e(
                Kind::Function,
                "p.oldName",
                "function oldName(a: string): number",
                "Parse a number out of text.",
            ),
            e(Kind::Function, "p.sig", "function sig(a: string): void", ""),
            e(Kind::Function, "p.dep", "function dep(): void", "Fine."),
            e(Kind::Function, "p.same", "function same(): void", ""),
            e(Kind::Function, "p.kept", "function kept(): void", ""),
        ];
        let mut kept_alias = e(Kind::Alias, "p.kept", "export { kept }", "");
        kept_alias.alias_of = Some("kept".into());
        let new = vec![
            e(
                Kind::Function,
                "p.newName",
                "function newName(a: string): number",
                "Parse a number out of text.",
            ),
            e(Kind::Function, "p.sig", "function sig(a: string, b: number): void", ""),
            e(Kind::Function, "p.dep", "function dep(): void", "@deprecated use other"),
            e(Kind::Function, "p.same", "function   same():  void", ""),
            kept_alias,
        ];
        let d = diff(&old, &new);
        let by = |p: &str| d.iter().find(|x| x.old.path == p).map(|x| x.change);
        assert_eq!(by("p.gone"), Some(Change::Removed));
        assert_eq!(by("p.oldName"), Some(Change::Renamed));
        assert_eq!(d.iter().find(|x| x.old.path == "p.oldName").unwrap().new.as_ref().unwrap().path, "p.newName");
        assert_eq!(by("p.sig"), Some(Change::SignatureChanged));
        assert_eq!(by("p.dep"), Some(Change::Deprecated));
        assert_eq!(by("p.same"), None, "whitespace is not a change");
        assert_eq!(by("p.kept"), None, "a re-export is not a removal");
    }

    #[test]
    fn mentions_respects_boundaries_and_members() {
        assert!(mentions("z.object({})", "object", true));
        assert!(!mentions("const objects = 1", "object", false));
        assert!(!mentions("object(1)", "object", true));
        assert!(mentions("use x::spawn;", "spawn", true));
        assert!(mentions("import { oldName } from 'p'", "oldName", false));
    }
}

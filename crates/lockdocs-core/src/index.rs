//! Per-package index: entries plus their term frequencies, cached on disk
//! by package, exact version and source location.

use crate::bm25;
use crate::extract::{self, Entry, Kind};
use crate::locate::{self, Source};
use crate::{cache, Dep, Eco};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Bump when extraction or the on-disk format changes.
pub const FORMAT: u32 = 4;

#[derive(Serialize, Deserialize)]
pub struct PackageIndex {
    pub format: u32,
    pub eco: Eco,
    pub name: String,
    /// The version whose files were indexed.
    pub version: String,
    pub source: String,
    pub fetched: bool,
    pub entries: Vec<Entry>,
    pub terms: Vec<Vec<(String, u16)>>,
    pub lens: Vec<u32>,
    pub build_ms: u64,
}

impl PackageIndex {
    pub fn id(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

/// Split text into (prose, code) where code is inside ``` fences.
fn split_code(doc: &str) -> (String, String) {
    let mut prose = String::new();
    let mut code = String::new();
    let mut in_fence = false;
    for l in doc.lines() {
        let t = l.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        let dst = if in_fence || l.starts_with("    ") && !l.trim().is_empty() && prose.ends_with("\n\n") {
            &mut code
        } else {
            &mut prose
        };
        dst.push_str(l);
        dst.push('\n');
    }
    (prose, code)
}

/// Weighted terms: names and headings count most, doc prose next, code
/// examples and signatures least (they repeat incidental identifiers).
pub fn entry_tf(e: &Entry) -> (Vec<(String, u16)>, u32) {
    let (prose, code) = split_code(&e.doc);
    if e.kind == Kind::Prose {
        let head = e.name.split(" › ").skip(1).collect::<Vec<_>>().join(" ");
        bm25::tf(&[(&head, 6), (&e.file, 1), (&prose, 2), (&code, 1)])
    } else {
        bm25::tf(&[(&e.name, 8), (&e.path, 2), (&e.sig, 1), (&prose, 2), (&code, 1)])
    }
}

/// Files whose change means the package was modified in place.
fn stamp(src: &Source) -> String {
    let probe: Vec<PathBuf> = match &src.metadata {
        Some(m) => vec![m.clone(), m.with_file_name("RECORD")],
        None => ["package.json", "Cargo.toml", "go.mod", ".lockdocs-complete"]
            .iter()
            .map(|f| src.dir.join(f))
            .collect(),
    };
    let mut s = String::new();
    for p in probe {
        if let Ok(m) = std::fs::metadata(&p) {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            s.push_str(&format!("{}:{t};", m.len()));
        }
    }
    s
}

fn cache_path(dep: &Dep, src: &Source, types: Option<&Path>) -> PathBuf {
    let key = cache::hash(&[
        &FORMAT.to_string(),
        dep.eco.as_str(),
        &dep.name,
        &src.version,
        &src.dir.to_string_lossy(),
        &stamp(src),
        &types.map(|t| t.to_string_lossy().to_string()).unwrap_or_default(),
    ]);
    let safe = format!("{}@{}", dep.name, src.version).replace(['/', '\\', ':'], "+");
    cache::dir().join("idx").join(dep.eco.as_str()).join(format!("{safe}-{key}.bin"))
}

/// `@types/<name>` next to an npm package that ships no declarations.
fn types_pkg(dep: &Dep, root: &Path) -> Option<PathBuf> {
    if dep.eco != Eco::Npm || dep.name.starts_with("@types/") {
        return None;
    }
    let mangled = match dep.name.strip_prefix('@') {
        Some(scoped) => scoped.replacen('/', "__", 1),
        None => dep.name.clone(),
    };
    locate::npm_installed(&format!("@types/{mangled}"), root).map(|(d, _)| d)
}

pub fn build(dep: &Dep, src: &Source, root: &Path) -> PackageIndex {
    let t = Instant::now();
    let types = types_pkg(dep, root);
    let entries = extract::extract(dep.eco, &dep.name, src, types.as_deref());
    let (terms, lens): (Vec<_>, Vec<_>) = entries.iter().map(entry_tf).unzip();
    PackageIndex {
        format: FORMAT,
        eco: dep.eco,
        name: dep.name.clone(),
        version: src.version.clone(),
        source: src.label.clone(),
        fetched: src.fetched,
        entries,
        terms,
        lens,
        build_ms: t.elapsed().as_millis() as u64,
    }
}

/// Load from the disk cache, or build and store.
pub fn load_or_build(dep: &Dep, src: &Source, root: &Path) -> PackageIndex {
    let types = types_pkg(dep, root);
    let path = cache_path(dep, src, types.as_deref());
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(idx) = postcard::from_bytes::<PackageIndex>(&bytes) {
            if idx.format == FORMAT {
                return idx;
            }
        }
    }
    let idx = build(dep, src, root);
    if let Ok(bytes) = postcard::to_stdvec(&idx) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
    idx
}

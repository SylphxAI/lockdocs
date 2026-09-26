//! Per-package index: entries plus their term frequencies, cached on disk
//! by package, exact version and source location.

use crate::bm25;
use crate::embed::{self, Vec8};
use crate::extract::{self, Entry, Kind};
use crate::locate::{self, Source};
use crate::upstream::Manifest;
use crate::{cache, Dep, Eco};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Bump when extraction or the on-disk format changes.
pub const FORMAT: u32 = 9;

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
    /// Terms of the heading or name plus the first sentence only, ranked on
    /// their own so a long body does not bury what the entry says it is.
    pub head: Vec<Vec<(String, u16)>>,
    pub head_lens: Vec<u32>,
    pub build_ms: u64,
    /// `github.com/o/r@tag (N files)` when upstream docs are included.
    pub upstream: Option<String>,
    /// Embedding model id, or empty for keyword-only.
    pub embed: String,
    /// One embedding per entry (empty without a model).
    pub vecs: Vec<Vec8>,
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

/// The first sentence of a doc comment.
pub fn summary(prose: &str) -> &str {
    let p = prose.trim_start();
    let end = [p.find(". "), p.find(".\n"), p.find("\n\n")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(p.len())
        .min(300);
    let mut end = end.min(p.len());
    while !p.is_char_boundary(end) {
        end -= 1;
    }
    &p[..end]
}

/// Weighted terms: names and headings count most, doc prose next, code
/// examples and signatures least (they repeat incidental identifiers).
pub fn entry_tf(e: &Entry) -> (Vec<(String, u16)>, u32) {
    let (prose, code) = split_code(&e.doc);
    if e.kind == Kind::Prose {
        let head = e.name.split(" › ").skip(1).collect::<Vec<_>>().join(" ");
        bm25::tf(&[(&head, 6), (&e.file, 1), (&prose, 2), (&code, 1)])
    } else {
        // The first sentence says what the symbol is for: weigh it like a heading.
        let summary = summary(&prose);
        bm25::tf(&[(&e.name, 8), (&e.path, 2), (&e.sig, 1), (summary, 4), (&prose, 2), (&code, 1)])
    }
}

/// Heading (or name) and first sentence: what an entry says it is about.
pub fn head_tf(e: &Entry) -> (Vec<(String, u16)>, u32) {
    let (prose, _) = split_code(&e.doc);
    let first = summary(&prose);
    if e.kind == Kind::Prose {
        bm25::tf(&[(crate::markdown::topic_heading(&e.name), 2), (first, 1)])
    } else {
        bm25::tf(&[(&e.name, 2), (first, 1)])
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

/// What an entry means, for the embedding: its name and the start of its
/// prose (code examples and long signatures dilute a mean-pooled vector).
pub fn embed_text(e: &Entry) -> String {
    let (prose, code) = split_code(&e.doc);
    let mut body: String = prose.chars().take(600).collect();
    if e.kind == Kind::Prose {
        // A code-only section embedded by its title alone matches short
        // questions far too well; its code says what it is about.
        if body.trim().is_empty() {
            body = code.chars().take(600).collect();
        }
        let head: Vec<&str> = e.name.split(" › ").collect();
        let tail = head[head.len().saturating_sub(2)..].join(" ");
        return format!("{tail}. {body}");
    }
    if body.trim().is_empty() {
        body = e.sig.chars().take(200).collect();
    }
    let parent = e.path.rsplit(['.', ':']).nth(1).unwrap_or("");
    format!("{} {parent}. {body}", e.name)
}

fn cache_path(dep: &Dep, src: &Source, types: Option<&Path>, up: Option<&(PathBuf, Manifest)>, embed: &str) -> PathBuf {
    let up_key = up.map(|(_, m)| format!("{}@{:?}:{}:{:?}", m.repo, m.tag, m.files, m.site)).unwrap_or_default();
    let key = cache::hash(&[
        embed,
        &up_key,
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

pub fn build(dep: &Dep, src: &Source, root: &Path, up: Option<&(PathBuf, Manifest)>) -> PackageIndex {
    let t = Instant::now();
    let types = types_pkg(dep, root);
    let up_dir = up.filter(|(_, m)| m.files > 0).map(|(d, _)| d.as_path());
    let entries = extract::extract(dep.eco, &dep.name, src, types.as_deref(), up_dir);
    let (terms, lens): (Vec<_>, Vec<_>) = entries.iter().map(entry_tf).unzip();
    let (head, head_lens): (Vec<_>, Vec<_>) = entries.iter().map(head_tf).unzip();
    let model = embed::get();
    let vecs: Vec<Vec8> = match &model {
        Some(m) => {
            use rayon::prelude::*;
            entries.par_iter().map(|e| m.embed8(&embed_text(e))).collect()
        }
        None => Vec::new(),
    };
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
        head,
        head_lens,
        build_ms: t.elapsed().as_millis() as u64,
        upstream: up
            .filter(|(_, m)| m.files > 0)
            .map(|(_, m)| format!("{}@{} ({} files)", m.repo, m.tag.as_deref().unwrap_or("?"), m.files)),
        embed: if model.is_some() { embed::MODEL_ID.to_string() } else { String::new() },
        vecs,
    }
}

/// Load from the disk cache, or build and store.
pub fn load_or_build(dep: &Dep, src: &Source, root: &Path, up: Option<&(PathBuf, Manifest)>) -> PackageIndex {
    let types = types_pkg(dep, root);
    let embed_id = if embed::get().is_some() { embed::MODEL_ID } else { "" };
    let path = cache_path(dep, src, types.as_deref(), up, embed_id);
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(idx) = postcard::from_bytes::<PackageIndex>(&bytes) {
            if idx.format == FORMAT {
                return idx;
            }
        }
    }
    let idx = build(dep, src, root, up);
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

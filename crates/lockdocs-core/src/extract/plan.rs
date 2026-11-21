//! Select package files, separately installed declarations, and upstream docs for extraction.

use crate::locate::Source;
use crate::Eco;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "compiled",
    ".git",
    "test",
    "tests",
    "__tests__",
    "__test__",
    "testdata",
    "test_data",
    "benches",
    "bench",
    "benchmarks",
    "fixtures",
    "__fixtures__",
    "__pycache__",
    "examples",
    "example",
    "_examples",
    "coverage",
    ".github",
    "vendor",
    "internal",
    "__snapshots__",
];

const MAX_FILES: usize = 25_000;
pub(super) const MAX_FILE_BYTES: u64 = 3 * 1024 * 1024;
pub(super) const EXAMPLE_EXTS: &[&str] = &["py", "ts", "tsx", "js", "jsx", "mjs", "rs", "go"];

pub(super) fn is_doc_name(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    [
        "README",
        "CHANGELOG",
        "CHANGES",
        "HISTORY",
        "MIGRATION",
        "MIGRATING",
        "UPGRADING",
        "UPGRADE",
        "RELEASES",
        "NEWS",
        "GUIDE",
        "API",
    ]
    .iter()
    .any(|p| u.starts_with(p))
}

fn doc_ext(ext: &str) -> bool {
    matches!(ext, "md" | "mdx" | "markdown" | "rst")
}

fn walk(dir: &Path, rel: &Path, depth: usize, out: &mut Vec<PathBuf>, public_npm: bool) {
    if depth > 14 || out.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(ft) = e.file_type() else { continue };
        let r = rel.join(&name);
        if ft.is_dir() {
            if (SKIP_DIRS.contains(&name.as_str()) && !(public_npm && name == "internal")) || name.starts_with('.') {
                continue;
            }
            walk(&e.path(), &r, depth + 1, out, public_npm);
        } else if ft.is_file() || ft.is_symlink() {
            out.push(r);
        }
        if out.len() >= MAX_FILES {
            return;
        }
    }
}

/// Every file under `dir` (docs trees keep folders like `examples/` we want).
pub(super) fn walk_all(dir: &Path, rel: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 16 || out.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let r = rel.join(&name);
        match e.file_type() {
            Ok(t) if t.is_dir() => walk_all(&e.path(), &r, depth + 1, out),
            Ok(_) => out.push(r),
            _ => {}
        }
    }
}

pub(super) fn ext_of(p: &Path) -> String {
    let s = p.file_name().map(|f| f.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    for e in [".d.ts", ".d.mts", ".d.cts"] {
        if s.ends_with(e) {
            return e[1..].to_string();
        }
    }
    p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

fn in_docs_dir(p: &Path) -> bool {
    p.components().any(|c| {
        matches!(
            c.as_os_str().to_string_lossy().to_ascii_lowercase().as_str(),
            "docs" | "doc" | "documentation" | "guide" | "guides"
        )
    })
}

/// A unit of work: one file to read, how to parse it, and how to label it.
pub(super) struct Job {
    pub(super) abs: PathBuf,
    pub(super) rel: String,
    pub(super) how: How,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum How {
    Prose,
    /// A docs page written as a JS/TSX component.
    Page,
    Example,
    Metadata,
    Ts,
    Python,
    Rust,
    Go,
}

/// Plan which files to read for a package.
pub(super) fn plan(eco: Eco, src: &Source, extra_types: Option<&Path>, upstream: Option<&Path>, public_api: bool) -> Vec<Job> {
    let mut rels = Vec::new();
    match &src.files {
        Some(list) => rels.extend(list.iter().cloned()),
        None => walk(&src.dir, Path::new(""), 0, &mut rels, public_api),
    }
    let mut jobs = Vec::new();
    let names: HashSet<String> = rels.iter().map(|r| r.to_string_lossy().replace('\\', "/")).collect();
    let mut has_dts = false;
    for r in &rels {
        let e = ext_of(r);
        if e.starts_with("d.") {
            has_dts = true;
        }
    }
    let rs_crate_src = eco == Eco::Cargo;
    for r in rels {
        let rel = r.to_string_lossy().replace('\\', "/");
        if rel
            .split('/')
            .any(|seg| SKIP_DIRS.contains(&seg) && !(public_api && seg == "internal") && src.files.is_some())
        {
            continue;
        }
        let abs = src.dir.join(&r);
        let fname = r.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let e = ext_of(&r);
        let depth = rel.matches('/').count();
        let how = if doc_ext(&e) || (e.is_empty() || e == "txt") && is_doc_name(&fname) && depth == 0 {
            if is_doc_name(&fname) || in_docs_dir(&r) || depth == 0 {
                Some(How::Prose)
            } else {
                None
            }
        } else {
            match eco {
                Eco::Npm => match e.as_str() {
                    "d.ts" => Some(How::Ts),
                    "d.mts" | "d.cts" => {
                        let base = rel.trim_end_matches(".d.mts").trim_end_matches(".d.cts");
                        (public_api || !names.contains(&format!("{base}.d.ts"))).then_some(How::Ts)
                    }
                    "ts" | "mts" | "cts" | "tsx" if !has_dts => Some(How::Ts),
                    "js" | "mjs" | "cjs" if !has_dts && !fname.contains(".min.") => Some(How::Ts),
                    _ => None,
                },
                Eco::PyPI => match e.as_str() {
                    "py" => Some(How::Python),
                    "pyi" => {
                        let base = rel.trim_end_matches(".pyi");
                        if names.contains(&format!("{base}.py")) {
                            None
                        } else {
                            Some(How::Python)
                        }
                    }
                    _ => None,
                },
                Eco::Cargo => (e == "rs" && rs_crate_src && rel.starts_with("src/")).then_some(How::Rust),
                Eco::Go => (e == "go" && !fname.ends_with("_test.go")).then_some(How::Go),
            }
        };
        if let Some(how) = how {
            jobs.push(Job { abs, rel, how });
        }
    }
    // Upstream docs fetched from the repository at this version's tag.
    if let Some(up) = upstream {
        let pages: HashSet<String> = std::fs::read_to_string(up.join(".lockdocs-upstream.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<crate::upstream::Manifest>(&t).ok())
            .map(|m| m.pages.into_iter().collect())
            .unwrap_or_default();
        let mut rels = Vec::new();
        walk_all(up, Path::new(""), 0, &mut rels);
        for r in rels {
            let e = ext_of(&r);
            let plain = r.to_string_lossy().replace('\\', "/");
            let rel = format!("upstream:{plain}");
            if pages.contains(&plain) {
                jobs.push(Job {
                    abs: up.join(&r),
                    rel,
                    how: How::Page,
                });
                continue;
            }
            // Django and others write reStructuredText in `.txt` files under docs/.
            if doc_ext(&e) || e == "mdoc" || e == "txt" {
                jobs.push(Job {
                    abs: up.join(&r),
                    rel,
                    how: How::Prose,
                });
            } else if EXAMPLE_EXTS.contains(&e.as_str()) {
                // Docs examples (`docs/examples/validators_simple.py`) carry the code the pages include.
                jobs.push(Job {
                    abs: up.join(&r),
                    rel,
                    how: How::Example,
                });
            }
        }
    }
    if let Some(meta) = &src.metadata {
        jobs.push(Job {
            abs: meta.clone(),
            rel: "README (package metadata)".into(),
            how: How::Metadata,
        });
    }
    // Types that ship separately (`@types/react` for `react`).
    if let (Some(types), false) = (extra_types, has_dts) {
        let mut rels = Vec::new();
        walk(types, Path::new(""), 0, &mut rels, public_api);
        let label = types
            .iter()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        for r in rels {
            if ext_of(&r) == "d.ts" || public_api && matches!(ext_of(&r).as_str(), "d.cts" | "d.mts") {
                let rel = format!("{label}/{}", r.to_string_lossy().replace('\\', "/"));
                jobs.push(Job {
                    abs: types.join(&r),
                    rel,
                    how: How::Ts,
                });
            }
        }
    }
    jobs
}

use super::CallSite;
use crate::extract::{Entry, Kind};
use crate::{norm_name, Eco};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
pub(super) fn mentions(line: &str, name: &str, member: bool) -> bool {
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

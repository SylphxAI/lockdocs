use super::{ancestors, cargo_home, src, tilde, Source};
use crate::Dep;
use std::path::{Path, PathBuf};

pub(super) fn locate(dep: &Dep, root: &Path) -> Option<Source> {
    let dirname = format!("{}-{}", dep.name, dep.version);
    for a in ancestors(root).into_iter().take(3) {
        for v in ["vendor", "third_party"] {
            for cand in [a.join(v).join(&dirname), a.join(v).join(&dep.name)] {
                if cand.join("Cargo.toml").is_file() {
                    return Some(src(cand, &a, dep.version.clone()));
                }
            }
        }
    }
    if dep.from.ends_with("(git)") {
        return cargo_git(dep);
    }
    let reg = cargo_home()?.join("registry").join("src");
    for e in std::fs::read_dir(&reg).ok()?.flatten() {
        let d = e.path().join(&dirname);
        if d.join("Cargo.toml").is_file() {
            return Some(Source {
                label: tilde(&d),
                dir: d,
                files: None,
                metadata: None,
                version: dep.version.clone(),
                fetched: false,
            });
        }
    }
    None
}

/// A git dependency checked out by Cargo under `$CARGO_HOME/git/checkouts`.
fn cargo_git(dep: &Dep) -> Option<Source> {
    let root = cargo_home()?.join("git").join("checkouts");
    let mut found = None;
    find_crate(&root, &dep.name, &dep.version, 0, &mut found);
    let d = found?;
    Some(Source {
        label: tilde(&d),
        dir: d,
        files: None,
        metadata: None,
        version: dep.version.clone(),
        fetched: false,
    })
}

fn find_crate(dir: &Path, name: &str, version: &str, depth: usize, found: &mut Option<PathBuf>) {
    if depth > 4 || found.is_some() {
        return;
    }
    if let Ok(t) = std::fs::read_to_string(dir.join("Cargo.toml")) {
        if let Ok(v) = toml::from_str::<toml::Table>(&t) {
            let pkg = v.get("package");
            let n = pkg.and_then(|p| p.get("name")).and_then(|n| n.as_str());
            let ver = pkg.and_then(|p| p.get("version")).and_then(|n| n.as_str());
            if n == Some(name) && ver == Some(version) {
                *found = Some(dir.to_path_buf());
                return;
            }
        }
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut subs: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let f = e.file_name();
            let f = f.to_string_lossy();
            f != "target" && !f.starts_with('.')
        })
        .map(|e| e.path())
        .collect();
    subs.sort();
    for s in subs {
        find_crate(&s, name, version, depth + 1, found);
    }
}

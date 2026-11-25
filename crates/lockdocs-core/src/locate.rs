//! Find a dependency's installed sources on this machine: node_modules,
//! Python site-packages, the Cargo registry and the Go module cache.

mod cargo;
mod go;
mod npm;
mod python;

pub use go::go_escape;
pub use npm::{npm_installed, pnp_zip_prefix};
pub use python::{dist_infos, python_source, site_packages};

use crate::{Dep, Eco};
use std::path::{Path, PathBuf};

/// Where a package's files are.
#[derive(Debug, Clone)]
pub struct Source {
    /// Package root (for Python: the site-packages directory).
    pub dir: PathBuf,
    /// Exact file list when the ecosystem records one (Python RECORD).
    pub files: Option<Vec<PathBuf>>,
    /// Python METADATA path (its body is the README).
    pub metadata: Option<PathBuf>,
    /// The version found on disk (may differ from the lockfile).
    pub version: String,
    /// Human label: `node_modules/zod`, `~/.cargo/registry/...`, `fetched: registry.npmjs.org`.
    pub label: String,
    pub fetched: bool,
}

/// Ancestor directories from `root` up to (but not past) the home directory.
fn ancestors(root: &Path) -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let mut out = Vec::new();
    for a in root.ancestors().take(8) {
        if Some(a.to_path_buf()) == home && !out.is_empty() {
            break;
        }
        out.push(a.to_path_buf());
    }
    out
}

pub fn locate(dep: &Dep, root: &Path) -> Option<Source> {
    match dep.eco {
        Eco::Npm => npm::locate(dep, root),
        Eco::PyPI => python::locate(dep, root),
        Eco::Cargo => cargo::locate(dep, root),
        Eco::Go => go::locate(dep, root),
    }
}

fn src(dir: PathBuf, base: &Path, version: String) -> Source {
    let label = dir.strip_prefix(base).map(|p| p.display().to_string()).unwrap_or_else(|_| tilde(&dir));
    Source {
        dir,
        files: None,
        metadata: None,
        version,
        label: label.replace('\\', "/"),
        fetched: false,
    }
}

pub fn tilde(p: &Path) -> String {
    if let Some(h) = dirs::home_dir() {
        if let Ok(r) = p.strip_prefix(&h) {
            return format!("~/{}", r.display()).replace('\\', "/");
        }
    }
    p.display().to_string().replace('\\', "/")
}

// Keep discovery environment reads here. The ecosystem modules call these
// helpers only when their lookup reaches the corresponding fallback.

fn global_yarn_cache() -> Option<PathBuf> {
    std::env::var_os("YARN_CACHE_FOLDER")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".yarn").join("berry").join("cache")))
}

fn no_system_python() -> bool {
    std::env::var("LOCKDOCS_NO_SYSTEM_PYTHON").is_ok()
}

fn active_python_prefixes() -> impl Iterator<Item = String> {
    ["VIRTUAL_ENV", "CONDA_PREFIX"].into_iter().filter_map(|var| std::env::var(var).ok())
}

fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))
}

fn go_modcache() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GOMODCACHE").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("GOPATH").filter(|p| !p.is_empty()) {
        let first = std::env::split_paths(&p).next()?;
        return Some(first.join("pkg").join("mod"));
    }
    dirs::home_dir().map(|h| h.join("go").join("pkg").join("mod"))
}

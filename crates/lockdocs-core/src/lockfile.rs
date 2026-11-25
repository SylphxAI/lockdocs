//! Lockfile and manifest parsers. Each parser is a pure function from file
//! contents to exact versions, so they are easy to test.

use crate::{Dep, Eco};
use std::path::Path;

mod cargo;
mod go;
mod js;
mod python;

#[cfg(test)]
mod tests;

pub use cargo::cargo_lock;
pub use go::go_mod;
pub use js::{bun_lock, npm_lock, package_json_direct, pnpm_lock, yarn_lock};
pub use python::{is_requirements, pipfile_lock, poetry_lock, pyproject_direct, requirements, uv_lock};

/// Lockfiles lockdocs reads, in priority order within an ecosystem.
pub const LOCKFILES: &[(&str, Eco)] = &[
    ("package-lock.json", Eco::Npm),
    ("npm-shrinkwrap.json", Eco::Npm),
    ("pnpm-lock.yaml", Eco::Npm),
    ("bun.lock", Eco::Npm),
    ("yarn.lock", Eco::Npm),
    ("Cargo.lock", Eco::Cargo),
    ("uv.lock", Eco::PyPI),
    ("poetry.lock", Eco::PyPI),
    ("pdm.lock", Eco::PyPI),
    ("Pipfile.lock", Eco::PyPI),
    ("go.mod", Eco::Go),
];

fn git_url(s: &str) -> bool {
    s.contains("git+")
        || s.contains("git://")
        || s.contains("git@")
        || s.contains("github:")
        || s.contains(".git#")
        || s.contains(".git?")
        || s.ends_with(".git")
}

fn dep(eco: Eco, name: &str, version: &str, direct: bool, from: &str) -> Dep {
    Dep {
        eco,
        name: name.to_string(),
        version: version.trim().to_string(),
        direct,
        from: from.to_string(),
    }
}

/// Parse one lockfile by file name. Returns `None` for unknown names.
pub fn parse(file_name: &str, text: &str, dir: &Path) -> Option<anyhow::Result<Vec<Dep>>> {
    let r = match file_name {
        "package-lock.json" | "npm-shrinkwrap.json" => npm_lock(text, file_name),
        "pnpm-lock.yaml" => pnpm_lock(text),
        "bun.lock" => bun_lock(text),
        "yarn.lock" => Ok(yarn_lock(text, &package_json_direct(dir))),
        "Cargo.lock" => cargo_lock(text),
        "uv.lock" => uv_lock(text),
        "poetry.lock" | "pdm.lock" => poetry_lock(text, file_name, &pyproject_direct(dir)),
        "Pipfile.lock" => pipfile_lock(text),
        "go.mod" => Ok(go_mod(text)),
        n if is_requirements(n) => Ok(requirements(text, n)),
        _ => return None,
    };
    Some(r)
}

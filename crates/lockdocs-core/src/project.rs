//! A project: its lockfiles and the exact dependency versions they pin.

use crate::{Dep, Eco};
use std::path::PathBuf;

mod load;
mod spec;

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    /// Lockfiles read, relative to `root` when inside it.
    pub lockfiles: Vec<String>,
    pub deps: Vec<Dep>,
    pub warnings: Vec<String>,
}

/// A package reference: `zod`, `zod@3.23.8`, `npm:zod`, `pypi:pydantic@2.9.2`,
/// `github.com/gin-gonic/gin`, `@tanstack/react-query@5`.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub eco: Option<Eco>,
    pub name: String,
    pub version: Option<String>,
}

impl Project {
    pub fn direct(&self) -> impl Iterator<Item = &Dep> {
        self.deps.iter().filter(|d| d.direct)
    }

    /// Dependencies matching a spec, best first (direct, then version match).
    pub fn find(&self, spec: &Spec) -> Vec<&Dep> {
        let mut v: Vec<&Dep> = self.deps.iter().filter(|d| spec.name_matches(d)).collect();
        v.sort_by_key(|d| (!spec.version_matches(&d.version), !d.direct));
        v
    }

    /// Names that look like `query` (for "did you mean").
    pub fn similar(&self, query: &str, n: usize) -> Vec<String> {
        let q = query.to_ascii_lowercase();
        let mut v: Vec<String> = self
            .deps
            .iter()
            .filter(|d| {
                let name = d.name.to_ascii_lowercase();
                name.contains(&q) || (q.len() >= 3 && q.contains(name.rsplit('/').next().unwrap_or(&name)))
            })
            .map(|d| format!("{}:{}@{}", d.eco, d.name, d.version))
            .collect();
        v.dedup();
        v.truncate(n);
        v
    }
}

#[cfg(test)]
mod tests;

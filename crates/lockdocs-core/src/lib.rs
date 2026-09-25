//! lockdocs core: read lockfiles, find each dependency's installed sources,
//! extract API reference and prose, and answer queries with BM25 under a
//! token budget. Everything is local unless fetching is explicitly enabled.

pub mod bm25;
pub mod cache;
pub mod extract;
pub mod fetch;
pub mod index;
pub mod locate;
pub mod lockfile;
pub mod markdown;
pub mod project;
pub mod query;
pub mod tokenize;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Package ecosystems lockdocs understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Eco {
    Npm,
    PyPI,
    Cargo,
    Go,
}

impl Eco {
    pub fn as_str(self) -> &'static str {
        match self {
            Eco::Npm => "npm",
            Eco::PyPI => "pypi",
            Eco::Cargo => "cargo",
            Eco::Go => "go",
        }
    }
    pub fn parse(s: &str) -> Option<Eco> {
        Some(match s.to_ascii_lowercase().as_str() {
            "npm" | "node" | "js" | "ts" => Eco::Npm,
            "pypi" | "pip" | "py" | "python" => Eco::PyPI,
            "cargo" | "crates" | "crate" | "rust" | "rs" => Eco::Cargo,
            "go" | "golang" => Eco::Go,
            _ => return None,
        })
    }
    pub fn all() -> [Eco; 4] {
        [Eco::Npm, Eco::PyPI, Eco::Cargo, Eco::Go]
    }
}

impl fmt::Display for Eco {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A resolved dependency: an exact version from a lockfile or an install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dep {
    pub eco: Eco,
    pub name: String,
    pub version: String,
    /// Declared by the project itself (not only transitive).
    pub direct: bool,
    /// Where the version came from, e.g. `package-lock.json` or `node_modules`.
    pub from: String,
}

impl Dep {
    pub fn id(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

/// Normalize a package name for comparison within an ecosystem.
pub fn norm_name(eco: Eco, name: &str) -> String {
    match eco {
        // PEP 503
        Eco::PyPI => {
            let mut out = String::with_capacity(name.len());
            let mut dash = false;
            for c in name.chars() {
                if c == '-' || c == '_' || c == '.' {
                    if !dash {
                        out.push('-');
                    }
                    dash = true;
                } else {
                    out.push(c.to_ascii_lowercase());
                    dash = false;
                }
            }
            out
        }
        Eco::Cargo => name.replace('_', "-").to_ascii_lowercase(),
        Eco::Npm => name.to_ascii_lowercase(),
        Eco::Go => name.to_string(),
    }
}

/// Rough token estimate used for budgets (about 3.6 characters per token for
/// mixed prose and code; tests in the benchmark use a real tokenizer).
pub fn est_tokens(s: &str) -> usize {
    (s.chars().count() * 10).div_ceil(36)
}

//! The three questions agents ask: which versions do I have (`resolve`),
//! what do the docs say about X (`docs`), and what exactly is this symbol
//! (`api`). Answers are token-budgeted and cite `package@version path:line`.

mod api;
mod docs;
mod entries;
mod packages;
mod ranking;
mod render;
mod workspace;

#[cfg(test)]
mod tests;

pub use ranking::redirects;
pub use render::render_entry;

use crate::index::PackageIndex;
use crate::project::Project;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const DEFAULT_TOKENS: usize = 1200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Allow downloading exact versions that are not installed.
    pub fetch: bool,
    /// Anonymous release-tag docs on first use; independent of registry downloads.
    pub upstream: bool,
    /// lockdocs Pro: read the private registries and git hosts the user has
    /// configured. Set by the binary after it has verified the licence; the
    /// core never decides this itself.
    pub private: bool,
}

impl Default for Options {
    fn default() -> Self {
        let mut fetch = std::env::var("LOCKDOCS_FETCH").is_ok_and(|v| matches!(v.as_str(), "1" | "true" | "yes"));
        let mut upstream = automatic_upstream(std::env::var("LOCKDOCS_FETCH").ok().as_deref(), std::env::var("LOCKDOCS_NO_UPSTREAM").is_ok());
        if std::env::var("LOCKDOCS_OFFLINE").is_ok_and(|v| v != "0") {
            fetch = false;
            upstream = false;
        }
        Options {
            fetch,
            upstream,
            private: false,
        }
    }
}

fn automatic_upstream(fetch: Option<&str>, disabled: bool) -> bool {
    !disabled && !fetch.is_some_and(|v| matches!(v, "0" | "false" | "no"))
}

pub struct Answer {
    pub text: String,
    pub json: Value,
}

pub struct Engine {
    pub project: Project,
    opts: Options,
    indexes: Mutex<HashMap<String, Arc<PackageIndex>>>,
    upstream_notes: Mutex<HashMap<String, String>>,
    /// Packages that needed a private source this engine was not licensed to read.
    private_blocked: Mutex<Vec<crate::private::PrivateRequired>>,
}

impl Engine {
    pub fn new(root: &Path, opts: Options) -> Engine {
        Engine {
            project: Project::load(root),
            opts,
            indexes: Mutex::new(HashMap::new()),
            upstream_notes: Mutex::new(HashMap::new()),
            private_blocked: Mutex::new(Vec::new()),
        }
    }

    /// The packages that needed a private source without Pro since the last
    /// call. The binary turns a non-empty list into the `pro_required` answer.
    pub fn take_private_blocked(&self) -> Vec<crate::private::PrivateRequired> {
        std::mem::take(&mut *self.private_blocked.lock().unwrap())
    }

    pub fn root(&self) -> &Path {
        &self.project.root
    }
}

/// Where the engine keeps its per-root state for long-running servers.
#[derive(Default)]
pub struct Workspace {
    engines: Mutex<HashMap<PathBuf, (u64, Arc<Engine>)>>,
}

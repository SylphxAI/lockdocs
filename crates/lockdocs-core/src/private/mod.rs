//! lockdocs Pro "private sources": read packages and docs from the private
//! registries and git hosts the user has already configured, with the user's
//! own credentials, each sent only to the host it belongs to.
//!
//! Nothing here decides whether the caller may use Pro: the binary checks the
//! licence and sets `Options::private`. This module only knows two things: how
//! to find the user's existing configuration (`.npmrc`, uv/pip config,
//! `.cargo/config.toml`, `GOPROXY`, `~/.netrc`, token variables,
//! `git credential fill`), and how to keep a credential on its own host:
//!
//! * a `Credential` carries the exact origin (scheme, host, port) it was
//!   configured for, and `Creds::for_url` returns it only for that origin;
//! * `Http` follows redirects itself, one hop at a time, and asks `Creds` again
//!   for every hop, so a redirect to another host loses the header;
//! * `Auth` and `Credential` print as `***`, and every error that reaches the
//!   user passes through `Creds::redact`.

mod auth;
mod http;
mod url;

pub mod cargo;
pub mod git;
pub mod go;
pub mod npm;
pub mod pypi;

#[cfg(test)]
pub(crate) mod testutil;

pub use auth::{netrc_for, parse_netrc, strip_userinfo, Auth, Credential, Creds};
pub use http::{Http, Reply};
pub(crate) use url::{b64_decode, b64_encode};
pub use url::{Origin, Url};

use crate::{Dep, Eco};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The feature name a Pro-less call reports in the `pro_required` answer.
pub const FEATURE: &str = "Private sources";

/// A request needed a private source and the licence does not cover it.
#[derive(Debug, Clone)]
pub struct PrivateRequired {
    pub package: String,
    /// The host (never a credential) the package lives on.
    pub host: String,
}

impl std::fmt::Display for PrivateRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is read from {}, a source outside the public defaults; reading private sources is part of lockdocs Pro",
            self.package, self.host
        )
    }
}

impl std::error::Error for PrivateRequired {}

/// The process environment, or a fixed one for tests.
#[derive(Clone)]
pub struct Env {
    vars: Option<HashMap<String, String>>,
    pub home: Option<PathBuf>,
}

impl Env {
    pub fn process() -> Env {
        Env {
            vars: None,
            home: dirs::home_dir(),
        }
    }
    pub fn fixed(vars: &[(&str, &str)], home: Option<&Path>) -> Env {
        Env {
            vars: Some(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()),
            home: home.map(Path::to_path_buf),
        }
    }
    /// A variable, `None` when unset or empty.
    pub fn get(&self, k: &str) -> Option<String> {
        let v = match &self.vars {
            Some(m) => m.get(k).cloned(),
            None => std::env::var(k).ok(),
        };
        v.filter(|v| !v.is_empty())
    }
    /// Every variable whose name starts with `prefix` (case-insensitive).
    pub fn with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let p = prefix.to_ascii_uppercase();
        let all: Vec<(String, String)> = match &self.vars {
            Some(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            None => std::env::vars().collect(),
        };
        let mut out: Vec<_> = all
            .into_iter()
            .filter(|(k, _)| k.to_ascii_uppercase().starts_with(&p) && !k.is_empty())
            .collect();
        out.sort();
        out
    }
    pub fn home_file(&self, rel: &str) -> Option<PathBuf> {
        self.home.as_ref().map(|h| h.join(rel))
    }
}

pub(crate) fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok()
}

// ---- plans ------------------------------------------------------------------

/// One place a package may be read from.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Registry, index or proxy base URL (no credentials in it).
    pub base: String,
    /// Not one of the public defaults.
    pub private: bool,
    /// The user's config or the lockfile says this package belongs here, so a
    /// public lookup would be wrong (or would leak the name).
    pub definitive: bool,
    /// The exact artifact URL the lockfile recorded, when it did.
    pub pinned: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub candidates: Vec<Candidate>,
    pub creds: Creds,
}

impl Plan {
    pub fn private_host(&self) -> Option<String> {
        self.candidates
            .iter()
            .find(|c| c.private)
            .and_then(|c| Url::parse(&c.base).map(|u| u.origin().host))
    }
    /// Needs a private source for sure: the first choice is private and definitive.
    pub fn definitely_private(&self) -> bool {
        self.candidates.first().is_some_and(|c| c.private && c.definitive)
    }
    pub fn any_private(&self) -> bool {
        self.candidates.iter().any(|c| c.private)
    }
}

/// Where `dep` would be read from, per the user's own configuration and lockfiles.
pub fn plan(dep: &Dep, root: &Path, env: &Env) -> Plan {
    match dep.eco {
        Eco::Npm => npm::plan(dep, root, env),
        Eco::PyPI => pypi::plan(dep, root, env),
        Eco::Cargo => cargo::plan(dep, root, env),
        Eco::Go => go::plan(dep, root, env),
    }
}

/// Does reading `dep` need a private source for sure (no network)?
pub fn needs_private(dep: &Dep, root: &Path) -> Option<PrivateRequired> {
    let p = plan(dep, root, &Env::process());
    p.definitely_private().then(|| PrivateRequired {
        package: dep.id(),
        host: p.private_host().unwrap_or_default(),
    })
}

/// How a private download ended.
pub enum Outcome {
    /// Unpacked into the target directory; `host` is where it came from.
    Done { host: String },
    /// No private candidate applied; the caller runs the public path. Carries
    /// the host of a private candidate that was skipped without Pro, so a
    /// public failure can be reported as "needs Pro".
    UsePublic { skipped: Option<PrivateRequired> },
}

/// Run the plan: private candidates first (when `allowed`), then hand over to
/// the public path. Without `allowed`, a definitively private package fails at
/// once with `PrivateRequired`, without any network.
pub fn fetch_into(dep: &Dep, root: &Path, allowed: bool, dir: &Path) -> Result<Outcome> {
    let env = Env::process();
    let plan = plan(dep, root, &env);
    if !plan.any_private() {
        return Ok(Outcome::UsePublic { skipped: None });
    }
    let required = PrivateRequired {
        package: dep.id(),
        host: plan.private_host().unwrap_or_default(),
    };
    if !allowed {
        if plan.definitely_private() {
            return Err(required.into());
        }
        return Ok(Outcome::UsePublic { skipped: Some(required) });
    }
    let http = Http::new(plan.creds.clone());
    let mut errors = Vec::new();
    for c in &plan.candidates {
        if !c.private {
            return Ok(Outcome::UsePublic { skipped: None });
        }
        let r = match dep.eco {
            Eco::Npm => npm::download(&http, dep, c, dir),
            Eco::PyPI => pypi::download(&http, dep, c, dir),
            Eco::Cargo => cargo::download(&http, dep, c, dir),
            Eco::Go => go::download(&http, &env, dep, c, dir),
        };
        match r {
            Ok(true) => {
                return Ok(Outcome::Done {
                    host: Url::parse(&c.base).map_or_else(|| "a private source".into(), |u| u.origin().to_string()),
                })
            }
            Ok(false) => errors.push(format!(
                "{}: not found",
                Url::parse(&c.base).map_or_else(|| "private source".into(), |u| u.origin().to_string())
            )),
            Err(e) => errors.push(plan.creds.redact(&format!("{e:#}"))),
        }
    }
    bail!("{}: not available from the configured private sources ({})", dep.id(), errors.join("; "))
}

//! npm private registries, from the user's own `.npmrc` files and
//! `NPM_CONFIG_*` variables, and from `package-lock.json`'s recorded URLs.

mod config;

#[cfg(test)]
mod tests;

pub use config::parse_npmrc;

use super::{Candidate, Env, Http, Plan, Url};
use crate::Dep;
use anyhow::{bail, Result};
use config::{config, credentials, is_public, lock_resolved, scope, DEFAULT};
use std::path::Path;

pub fn plan(dep: &Dep, root: &Path, env: &Env) -> Plan {
    let cfg = config(root, env);
    let reg = scope(&dep.name)
        .and_then(|s| cfg.get(&format!("{s}:registry")))
        .or_else(|| cfg.get("registry"))
        .cloned()
        .unwrap_or_else(|| DEFAULT.to_string())
        .trim_end_matches('/')
        .to_string();
    let mut registries: Vec<String> = cfg
        .iter()
        .filter(|(k, _)| k.as_str() == "registry" || k.ends_with(":registry"))
        .map(|(_, v)| v.clone())
        .collect();
    registries.push(reg.clone());
    let pinned = lock_resolved(dep, root);
    if let Some(p) = &pinned {
        registries.push(p.clone());
    }
    let creds = credentials(&cfg, &registries);
    let mut candidates = Vec::new();
    if let Some(p) = pinned {
        if !is_public(&p) {
            let base = Url::parse(&p).map(|u| u.origin().to_string()).unwrap_or_default();
            candidates.push(Candidate {
                base,
                private: true,
                definitive: true,
                pinned: Some(p),
            });
        } else {
            // `package-lock.json` resolved this package to the public registry:
            // honour the resolved URL. A configured private registry must not
            // shadow the artifact the lockfile actually pinned.
            let Some(u) = Url::parse(&p) else { return Plan { candidates, creds } };
            candidates.push(Candidate {
                base: u.origin().to_string(),
                private: false,
                definitive: true,
                pinned: Some(p),
            });
        }
    } else if !is_public(&reg) {
        candidates.push(Candidate {
            base: reg,
            private: true,
            // A scoped registry is the user saying "this scope lives there".
            definitive: scope(&dep.name).is_some_and(|s| cfg.contains_key(&format!("{s}:registry"))),
            pinned: None,
        });
    }
    if candidates.first().is_none_or(|c| !c.definitive) {
        candidates.push(Candidate {
            base: DEFAULT.into(),
            private: false,
            definitive: false,
            pinned: None,
        });
    }
    Plan { candidates, creds }
}

/// Download one version from a private registry. Ok(false) when it has none.
pub fn download(http: &Http, dep: &Dep, c: &Candidate, dir: &Path) -> Result<bool> {
    let tarball = match &c.pinned {
        Some(p) => p.clone(),
        None => {
            let url = format!("{}/{}/{}", c.base.trim_end_matches('/'), dep.name.replace('/', "%2F"), dep.version);
            let Some(body) = http.get_ok(&url, Some("application/json"))? else {
                return Ok(false);
            };
            let meta: serde_json::Value = serde_json::from_slice(&body)?;
            match meta.pointer("/dist/tarball").and_then(|t| t.as_str()) {
                Some(t) => t.to_string(),
                None => bail!("{}: the registry returned no tarball for this version", dep.id()),
            }
        }
    };
    let Some(bytes) = http.get_ok(&tarball, None)? else { return Ok(false) };
    crate::fetch::untar_gz(&bytes, dir)?;
    Ok(true)
}

//! Read upstream cache entries and decide whether to refresh or retain them.

use std::path::PathBuf;

use anyhow::Result;

use crate::{cache, Dep};

use super::{Manifest, FORMAT};

pub fn dir(dep: &Dep) -> PathBuf {
    let safe = format!("{}@{}", dep.name, dep.version).replace(['/', '\\', ':'], "+");
    cache::dir().join("upstream").join(dep.eco.as_str()).join(safe)
}

/// Was this copy made by an older lockdocs that downloaded less?
pub fn stale(m: &Manifest) -> bool {
    m.format < FORMAT
}

/// A prior explicit enrichment is also usable by automatic and offline queries.
pub fn needs_refresh(m: &Manifest, explicit: bool) -> bool {
    if explicit {
        stale(m) || !m.docs_sites_checked
    } else {
        !compatible(m)
    }
}

/// Oldest cache format whose commit was resolved from an exact release tag.
/// Newer formats only differ in how the files were downloaded, so automatic
/// and offline queries keep trusting them; `stale` still lets a fetch refresh.
const MIN_TRUSTED_FORMAT: u32 = 4;

fn compatible(m: &Manifest) -> bool {
    m.format >= MIN_TRUSTED_FORMAT && (m.commit.is_some() || m.site.is_some() || m.files == 0)
}

/// A previous fetch (with or without files). Never touches the network.
pub fn cached(dep: &Dep) -> Option<(PathBuf, Manifest)> {
    let d = dir(dep);
    let m: Manifest = serde_json::from_str(&std::fs::read_to_string(d.join(".lockdocs-upstream.json")).ok()?).ok()?;
    Some((d, m))
}

pub(super) fn enrich_cached(prior: Option<(PathBuf, Manifest)>, explicit: bool, attempt: impl FnOnce() -> Result<Manifest>) -> Result<Manifest> {
    if let Some((_, m)) = prior.as_ref().filter(|(_, m)| !needs_refresh(m, explicit)) {
        return Ok(m.clone());
    }
    match attempt() {
        Ok(m) => Ok(m),
        Err(e) => match prior.filter(|(_, m)| compatible(m)) {
            Some((_, mut m)) => {
                m.note = Some(format!("upstream refresh failed: {e:#}; previous complete cache retained"));
                Ok(m)
            }
            None => Err(e),
        },
    }
}

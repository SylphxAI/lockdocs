use super::super::{Candidate, Http, Url};
use crate::Dep;
use anyhow::{bail, Result};
use std::path::Path;

fn prefix(name: &str) -> String {
    match name.len() {
        1 => "1".into(),
        2 => "2".into(),
        3 => format!("3/{}", &name[..1]),
        _ => format!("{}/{}", &name[..2], &name[2..4]),
    }
}

/// The download URL from the registry's `dl` template.
pub fn dl_url(dl: &str, krate: &str, version: &str) -> Option<String> {
    if !dl.contains(['{']) {
        return Some(format!("{}/{krate}/{version}/download", dl.trim_end_matches('/')));
    }
    let out = dl
        .replace("{crate}", krate)
        .replace("{version}", version)
        .replace("{prefix}", &prefix(krate))
        .replace("{lowerprefix}", &prefix(&krate.to_ascii_lowercase()));
    (!out.contains("{sha256-checksum}")).then_some(out)
}

/// Download one version from a sparse registry. Ok(false) when it has none.
pub fn download(http: &Http, dep: &Dep, c: &Candidate, dir: &Path) -> Result<bool> {
    if c.pinned.as_deref() == Some("git-index") {
        bail!("this registry uses the git index protocol; lockdocs reads sparse registries (`sparse+https://...`)");
    }
    let base = c.base.trim_end_matches('/');
    let Some(cfg) = http.get_ok(&format!("{base}/config.json"), None)? else {
        return Ok(false);
    };
    let cfg: serde_json::Value = serde_json::from_slice(&cfg)?;
    let Some(dl) = cfg.get("dl").and_then(|d| d.as_str()) else {
        bail!("the registry's config.json has no `dl` template");
    };
    let Some(url) = dl_url(dl, &dep.name, &dep.version) else {
        bail!("the registry's `dl` template uses {{sha256-checksum}}, which lockdocs does not read yet");
    };
    let url = if url.starts_with('/') {
        Url::parse(base).and_then(|b| b.join(&url)).map_or(url.clone(), |u| u.clean())
    } else {
        url
    };
    let Some(bytes) = http.get_ok(&url, None)? else { return Ok(false) };
    crate::fetch::untar_gz(&bytes, dir)?;
    Ok(true)
}

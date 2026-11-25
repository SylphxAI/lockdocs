//! Opt-in network fetch of one exact package version from its registry
//! (npm tarball, PyPI wheel or sdist, crates.io .crate, Go module proxy zip),
//! unpacked into the disk cache. Nothing here runs unless fetching is enabled.

mod archive;
#[cfg(test)]
mod tests;

use crate::locate::{python_source, tilde, Source};
use crate::{Dep, Eco};
use anyhow::{bail, Context, Result};
#[allow(unused_imports, reason = "Preserve the crate-level archive API")]
pub(crate) use archive::{safe_rel, untar_gz, untar_gz_sub, unzip, unzip_with, wanted, Limits, LIMITS};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .user_agent(concat!("lockdocs/", env!("CARGO_PKG_VERSION"), " (+https://github.com/SylphxAI/lockdocs)"))
        .build()
        .into()
}

fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let mut res = agent().get(url).call().with_context(|| format!("GET {url}"))?;
    let mut buf = Vec::new();
    res.body_mut().as_reader().take(MAX_DOWNLOAD + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_DOWNLOAD {
        bail!("{url} is larger than {} MB", MAX_DOWNLOAD / 1024 / 1024);
    }
    Ok(buf)
}

fn get_json(url: &str) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&get_bytes(url)?)?)
}

/// Where a fetched copy of `dep` is stored, keyed by the resolved source so a
/// package pinned to a private registry never shares a directory with the
/// public one of the same `name@version` (nor with another private registry's).
/// Two registries publishing the same version otherwise yield whichever copy
/// was fetched first.
fn slot(dep: &Dep, source: &str) -> PathBuf {
    let safe = format!("{}@{}", dep.name, dep.version).replace(['/', '\\', ':'], "+");
    let key = crate::cache::hash(&[dep.eco.as_str(), &dep.name, &dep.version, source]);
    crate::cache::dir().join("src").join(dep.eco.as_str()).join(format!("{safe}-{key}"))
}

/// The public default registry for a package's ecosystem.
fn public_default(dep: &Dep) -> &'static str {
    match dep.eco {
        Eco::Npm => "registry.npmjs.org",
        Eco::PyPI => "pypi.org",
        Eco::Cargo => "crates.io",
        Eco::Go => "proxy.golang.org",
    }
}

/// The source identity a fetch would resolve to: the private candidate the
/// user's configuration and lockfiles select, else the public default. Offline
/// and cheap; the plan is already read by the private path.
fn source_identity(dep: &Dep, root: Option<&Path>) -> String {
    let Some(root) = root else { return public_default(dep).into() };
    let plan = crate::private::plan(dep, root, &crate::private::Env::process());
    plan.candidates
        .first()
        .filter(|c| c.private)
        .map(|c| c.base.clone())
        .unwrap_or_else(|| public_default(dep).into())
}

/// Directory for a fetched package, for the public default source. Callers
/// that resolved a private source use [`fetched_dir_for`].
pub fn fetched_dir(dep: &Dep) -> PathBuf {
    slot(dep, public_default(dep))
}

/// Directory for a fetched `dep` read from `root`'s resolved source.
pub fn fetched_dir_for(dep: &Dep, root: Option<&Path>) -> PathBuf {
    let id = source_identity(dep, root);
    slot(dep, &id)
}

/// A previously fetched copy, if any (never touches the network). `root` lets
/// the lookup see the project's configured source, so a cached public copy is
/// never returned for a package whose lockfile pins a private registry.
pub fn cached_for(dep: &Dep, root: Option<&Path>) -> Option<Source> {
    if is_git(dep) {
        return None;
    }
    let dir = fetched_dir_for(dep, root);
    if dir.join(".lockdocs-complete").is_file() {
        return Some(source_for(dep, &dir));
    }
    None
}

fn source_for(dep: &Dep, dir: &Path) -> Source {
    let origin = match dep.eco {
        Eco::Npm => "registry.npmjs.org",
        Eco::PyPI => "pypi.org",
        Eco::Cargo => "crates.io",
        Eco::Go => "proxy.golang.org",
    };
    source_from(dep, dir, origin)
}

fn source_from(dep: &Dep, dir: &Path, origin: &str) -> Source {
    if dep.eco == Eco::PyPI {
        // A wheel unpacks to a site-packages layout with a dist-info RECORD.
        if let Some(di) = std::fs::read_dir(dir)
            .ok()
            .and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "dist-info")))
        {
            let mut s = python_source(dir, &di, dep.version.clone());
            s.label = format!("fetched from {origin} ({})", tilde(dir));
            s.fetched = true;
            return s;
        }
    }
    Source {
        dir: dir.to_path_buf(),
        files: None,
        metadata: None,
        version: dep.version.clone(),
        label: format!("fetched from {origin} ({})", tilde(dir)),
        fetched: true,
    }
}

/// Download and unpack `dep` if it is not cached yet.
pub fn is_git(dep: &Dep) -> bool {
    dep.from.ends_with("(git)")
}

/// Shared guard: a git checkout is not interchangeable with a registry release tag.
pub fn require_registry_origin(dep: &Dep) -> Result<()> {
    if is_git(dep) {
        bail!(
            "{} is a git dependency; use its resolved checkout files, not registry/release-tag docs",
            dep.id()
        );
    }
    Ok(())
}

pub fn fetch(dep: &Dep) -> Result<Source> {
    fetch_for(dep, None, false)
}

/// `fetch`, reading the project's private-source configuration when `root` is
/// given. Without `private` (lockdocs Pro), a package that needs a private
/// source fails with `private::PrivateRequired` and nothing is sent anywhere.
pub fn fetch_for(dep: &Dep, root: Option<&Path>, private: bool) -> Result<Source> {
    require_registry_origin(dep)?;
    if let Some(s) = cached_for(dep, root) {
        return Ok(s);
    }
    let dir = fetched_dir_for(dep, root);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let mut skipped = None;
    if let Some(root) = root {
        match crate::private::fetch_into(dep, root, private, &dir) {
            Ok(crate::private::Outcome::Done { host }) => {
                std::fs::write(dir.join(".lockdocs-complete"), b"")?;
                return Ok(source_from(dep, &dir, &format!("private source {host}")));
            }
            Ok(crate::private::Outcome::UsePublic { skipped: s }) => skipped = s,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
        }
    }
    let r = match dep.eco {
        Eco::Npm => {
            let meta = get_json(&format!("https://registry.npmjs.org/{}/{}", dep.name.replace('/', "%2F"), dep.version))?;
            let url = meta
                .pointer("/dist/tarball")
                .and_then(|t| t.as_str())
                .context("npm: no tarball for this version")?;
            untar_gz(&get_bytes(url)?, &dir)
        }
        Eco::Cargo => {
            let url = format!("https://static.crates.io/crates/{0}/{0}-{1}.crate", dep.name, dep.version);
            untar_gz(&get_bytes(&url)?, &dir)
        }
        Eco::Go => {
            let m = crate::locate::go_escape(&dep.name);
            let v = crate::locate::go_escape(&dep.version);
            let prefix = format!("{}@{}/", dep.name, dep.version);
            unzip(&get_bytes(&format!("https://proxy.golang.org/{m}/@v/{v}.zip"))?, &dir, false, Some(&prefix))
        }
        Eco::PyPI => {
            let meta = get_json(&format!("https://pypi.org/pypi/{}/{}/json", dep.name, dep.version))?;
            let urls = meta.get("urls").and_then(|u| u.as_array()).cloned().unwrap_or_default();
            let pick = |kind: &str, pure: bool| {
                urls.iter().find(|u| {
                    u.get("packagetype").and_then(|p| p.as_str()) == Some(kind)
                        && (!pure || u.get("filename").and_then(|f| f.as_str()).is_some_and(|f| f.ends_with("-none-any.whl")))
                })
            };
            let chosen = pick("bdist_wheel", true)
                .or_else(|| pick("bdist_wheel", false))
                .or_else(|| pick("sdist", false))
                .context("pypi: no files for this version")?;
            let url = chosen.get("url").and_then(|u| u.as_str()).context("pypi: no url")?;
            // The advertised filename, not the URL: pypi serves artifacts from
            // files.pythonhosted.org behind a `?`-signed query string.
            let filename = chosen.get("filename").and_then(|f| f.as_str()).unwrap_or(url);
            let bytes = get_bytes(url)?;
            crate::private::pypi::unpack(filename, &bytes, &dir)
        }
    };
    if let Err(e) = r {
        let _ = std::fs::remove_dir_all(&dir);
        // The public registry does not have it, and a private source was configured
        // for it: the answer is "that needs Pro", not "not found".
        return Err(match skipped {
            Some(req) => req.into(),
            None => e,
        });
    }
    std::fs::write(dir.join(".lockdocs-complete"), b"")?;
    Ok(source_for(dep, &dir))
}

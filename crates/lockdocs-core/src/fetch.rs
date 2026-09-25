//! Opt-in network fetch of one exact package version from its registry
//! (npm tarball, PyPI wheel or sdist, crates.io .crate, Go module proxy zip),
//! unpacked into the disk cache. Nothing here runs unless fetching is enabled.

use crate::locate::{python_source, tilde, Source};
use crate::{Dep, Eco};
use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;
const MAX_FILE: u64 = 8 * 1024 * 1024;

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

/// Directory for a fetched package.
pub fn fetched_dir(dep: &Dep) -> PathBuf {
    let safe = format!("{}@{}", dep.name, dep.version).replace(['/', '\\', ':'], "+");
    crate::cache::dir().join("src").join(dep.eco.as_str()).join(safe)
}

/// A previously fetched copy, if any (never touches the network).
pub fn cached(dep: &Dep) -> Option<Source> {
    let dir = fetched_dir(dep);
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
pub fn fetch(dep: &Dep) -> Result<Source> {
    if let Some(s) = cached(dep) {
        return Ok(s);
    }
    let dir = fetched_dir(dep);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
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
            let bytes = get_bytes(url)?;
            if url.ends_with(".whl") || url.ends_with(".zip") {
                unzip(&bytes, &dir, url.ends_with(".zip"), None)
            } else {
                untar_gz(&bytes, &dir)
            }
        }
    };
    if let Err(e) = r {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    std::fs::write(dir.join(".lockdocs-complete"), b"")?;
    Ok(source_for(dep, &dir))
}

/// Keep only paths that stay inside the target; drop the first component
/// (`package/`, `name-1.0/`) of tarballs.
pub(crate) fn safe_rel(p: &Path, strip_first: bool) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for (i, c) in p.components().enumerate() {
        match c {
            Component::Normal(s) => {
                if i == 0 && strip_first {
                    continue;
                }
                out.push(s)
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(crate) fn wanted(p: &Path) -> bool {
    let s = p.to_string_lossy().to_ascii_lowercase();
    if s.contains("node_modules/") || s.contains("/test/") || s.contains("/tests/") {
        return false;
    }
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(
        ext.as_str(),
        "md" | "mdx" | "markdown" | "rst" | "txt" | "ts" | "mts" | "cts" | "js" | "mjs" | "cjs" | "py" | "pyi" | "rs" | "go" | "json" | "toml" | "mod" | ""
    ) || s.ends_with("metadata")
        || s.ends_with("record")
}

fn untar_gz(bytes: &[u8], dir: &Path) -> Result<()> {
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for e in ar.entries()? {
        let mut e = e?;
        if !e.header().entry_type().is_file() || e.size() > MAX_FILE {
            continue;
        }
        let path = e.path()?.to_path_buf();
        let Some(rel) = safe_rel(&path, true) else { continue };
        if !wanted(&rel) {
            continue;
        }
        let out = dir.join(rel);
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut buf = Vec::new();
        e.read_to_end(&mut buf)?;
        std::fs::write(out, buf)?;
    }
    Ok(())
}

pub(crate) fn unzip(bytes: &[u8], dir: &Path, strip_first: bool, prefix: Option<&str>) -> Result<()> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        if !f.is_file() || f.size() > MAX_FILE {
            continue;
        }
        let Some(mut name) = f.enclosed_name() else { continue };
        if let Some(pre) = prefix {
            let Some(rest) = f.name().strip_prefix(pre) else { continue };
            name = PathBuf::from(rest);
        }
        let Some(rel) = safe_rel(&name, strip_first) else { continue };
        if !wanted(&rel) {
            continue;
        }
        let out = dir.join(rel);
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        std::fs::write(out, buf)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_escaping_paths() {
        assert_eq!(safe_rel(Path::new("package/README.md"), true), Some(PathBuf::from("README.md")));
        assert_eq!(safe_rel(Path::new("package/../../etc/passwd"), true), None);
        assert_eq!(safe_rel(Path::new("/abs/x"), false), None);
    }
}

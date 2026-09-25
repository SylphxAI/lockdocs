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
    if dep.from.ends_with("(git)") {
        bail!("{} is a git dependency; run `cargo fetch` to check it out", dep.id());
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

/// Zip-bomb guards: per file, total unpacked bytes, and entries examined.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub file: u64,
    pub total: u64,
    pub entries: usize,
}

pub(crate) const LIMITS: Limits = Limits {
    file: MAX_FILE,
    total: 256 * 1024 * 1024,
    entries: 100_000,
};

struct Budget {
    limits: Limits,
    total: u64,
    entries: usize,
}

impl Budget {
    fn new(limits: Limits) -> Budget {
        Budget { limits, total: 0, entries: 0 }
    }
    fn entry(&mut self) -> Result<()> {
        self.entries += 1;
        if self.entries > self.limits.entries {
            bail!("archive has more than {} entries", self.limits.entries);
        }
        Ok(())
    }
    /// Read one member without trusting its declared size.
    fn read(&mut self, r: &mut dyn Read) -> Result<Option<Vec<u8>>> {
        let mut buf = Vec::new();
        r.take(self.limits.file + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > self.limits.file {
            return Ok(None);
        }
        self.total += buf.len() as u64;
        if self.total > self.limits.total {
            bail!("archive unpacks to more than {} MB", self.limits.total / 1024 / 1024);
        }
        Ok(Some(buf))
    }
}

fn write_member(dir: &Path, rel: PathBuf, buf: Vec<u8>) -> Result<()> {
    let out = dir.join(rel);
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(out, buf)?;
    Ok(())
}

fn untar_gz(bytes: &[u8], dir: &Path) -> Result<()> {
    untar_gz_with(bytes, dir, LIMITS)
}

fn untar_gz_with(bytes: &[u8], dir: &Path, limits: Limits) -> Result<()> {
    let mut budget = Budget::new(limits);
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for e in ar.entries()? {
        budget.entry()?;
        let mut e = e?;
        if !e.header().entry_type().is_file() || e.size() > limits.file {
            continue;
        }
        let path = e.path()?.to_path_buf();
        let Some(rel) = safe_rel(&path, true) else { continue };
        if !wanted(&rel) {
            continue;
        }
        if let Some(buf) = budget.read(&mut e)? {
            write_member(dir, rel, buf)?;
        }
    }
    Ok(())
}

pub(crate) fn unzip(bytes: &[u8], dir: &Path, strip_first: bool, prefix: Option<&str>) -> Result<()> {
    unzip_with(bytes, dir, strip_first, prefix, LIMITS)
}

pub(crate) fn unzip_with(bytes: &[u8], dir: &Path, strip_first: bool, prefix: Option<&str>, limits: Limits) -> Result<()> {
    let mut budget = Budget::new(limits);
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    if z.len() > limits.entries {
        bail!("archive has more than {} entries", limits.entries);
    }
    for i in 0..z.len() {
        budget.entry()?;
        let mut f = z.by_index(i)?;
        if !f.is_file() || f.size() > limits.file {
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
        if let Some(buf) = budget.read(&mut f)? {
            write_member(dir, rel, buf)?;
        }
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

    fn zip_of(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn archive_bombs_are_rejected() {
        let dir = std::env::temp_dir().join(format!("lockdocs-bomb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let tiny = Limits {
            file: 1000,
            total: 2500,
            entries: 5,
        };
        // Highly compressible members: total unpacked size is capped.
        let big: Vec<(&str, Vec<u8>)> = vec![("a.md", vec![b'x'; 900]), ("b.md", vec![b'x'; 900]), ("c.md", vec![b'x'; 900])];
        let err = unzip_with(&zip_of(&big), &dir, false, None, tiny).unwrap_err();
        assert!(err.to_string().contains("unpacks to more than"), "{err}");
        // Entry count is capped.
        let many: Vec<(String, Vec<u8>)> = (0..6).map(|i| (format!("f{i}.md"), b"x".to_vec())).collect();
        let many: Vec<(&str, Vec<u8>)> = many.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
        let err = unzip_with(&zip_of(&many), &dir, false, None, tiny).unwrap_err();
        assert!(err.to_string().contains("more than 5 entries"), "{err}");
        // An oversized member is skipped, the rest unpacks.
        let mixed: Vec<(&str, Vec<u8>)> = vec![("huge.md", vec![b'x'; 5000]), ("ok.md", b"hello".to_vec())];
        unzip_with(&zip_of(&mixed), &dir, false, None, tiny).unwrap();
        assert!(dir.join("ok.md").is_file() && !dir.join("huge.md").exists());
        // tar.gz: the same total cap.
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()));
        for n in ["package/a.md", "package/b.md", "package/c.md"] {
            let data = vec![b'y'; 900];
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, n, &data[..]).unwrap();
        }
        let gz = tar.into_inner().unwrap().finish().unwrap();
        let err = untar_gz_with(&gz, &dir.join("t"), tiny).unwrap_err();
        assert!(err.to_string().contains("unpacks to more than"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

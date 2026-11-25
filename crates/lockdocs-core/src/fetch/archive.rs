//! Bounded tar.gz and ZIP extraction for fetched package files.

use anyhow::{bail, Result};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_FILE: u64 = 8 * 1024 * 1024;

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

pub(crate) fn untar_gz(bytes: &[u8], dir: &Path) -> Result<()> {
    untar_gz_with(bytes, dir, LIMITS)
}

/// Like `untar_gz`, keeping only the files under `sub` (relative to the
/// archive's top folder) and unpacking them without that prefix.
pub(crate) fn untar_gz_sub(bytes: &[u8], dir: &Path, sub: Option<&str>) -> Result<()> {
    untar_gz_prefix(bytes, dir, LIMITS, sub)
}

pub(super) fn untar_gz_with(bytes: &[u8], dir: &Path, limits: Limits) -> Result<()> {
    untar_gz_prefix(bytes, dir, limits, None)
}

fn untar_gz_prefix(bytes: &[u8], dir: &Path, limits: Limits, sub: Option<&str>) -> Result<()> {
    let mut budget = Budget::new(limits);
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for e in ar.entries()? {
        budget.entry()?;
        let mut e = e?;
        if !e.header().entry_type().is_file() || e.size() > limits.file {
            continue;
        }
        let path = e.path()?.to_path_buf();
        let Some(mut rel) = safe_rel(&path, true) else { continue };
        if let Some(sub) = sub {
            let Ok(inner) = rel.strip_prefix(sub) else { continue };
            rel = inner.to_path_buf();
            if rel.as_os_str().is_empty() {
                continue;
            }
        }
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

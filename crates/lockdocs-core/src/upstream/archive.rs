//! Stream repository archives within limits and extract regular files.

use std::io::Read;

use anyhow::{bail, Context, Result};

use super::client::{enc_path, Client};
use super::{Repo, MAX_FILE, MAX_FILES};

/// Compressed bytes downloaded from codeload by this process (for reporting).
static DOWNLOADED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Counts compressed bytes and stops the stream (with an error) past `cap`.
struct Capped<R> {
    inner: R,
    seen: u64,
    cap: u64,
    exceeded: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl<R: Read> Read for Capped<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.seen += n as u64;
        DOWNLOADED.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
        if self.seen > self.cap {
            self.exceeded.store(true, std::sync::atomic::Ordering::Relaxed);
            return Err(std::io::Error::other("archive exceeds the download cap"));
        }
        Ok(n)
    }
}

/// An archive stream plus whether it was cut for being over the cap.
pub(super) struct Archive {
    pub(super) body: Box<dyn Read>,
    pub(super) exceeded: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Archive {
    fn new(body: impl Read + 'static, cap: u64) -> Self {
        let exceeded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        Archive {
            body: Box::new(Capped {
                inner: body,
                seen: 0,
                cap,
                exceeded: exceeded.clone(),
            }),
            exceeded,
        }
    }
}

/// Stream `https://codeload.github.com/<owner>/<repo>/tar.gz/<ref>` (a tag, a
/// branch or a commit). Ok(None) on 404. Not governed by the REST API quota.
pub(super) fn codeload(agent: &Client, repo: &Repo, reference: &str) -> Result<Option<Archive>> {
    let sent_token = agent.source_header(&archive_url(agent, repo, reference), false).is_some();
    match codeload_once(agent, repo, reference, sent_token)? {
        // Only reachable on the future credentialed path: a token codeload does not
        // accept falls back to an anonymous request, which public repositories answer.
        None if sent_token => codeload_once(agent, repo, reference, false),
        other => Ok(other),
    }
}

/// Where the archive of `reference` lives: codeload on github.com, the host's
/// own archive endpoint elsewhere.
fn archive_url(agent: &Client, repo: &Repo, reference: &str) -> String {
    match &agent.git_host {
        Some(h) => h.archive_url(&format!("{}/{}", repo.owner, repo.name), reference),
        None => format!("{}/{}/{}/tar.gz/{}", agent.hosts.codeload, repo.owner, repo.name, enc_path(reference)),
    }
}

fn codeload_once(agent: &Client, repo: &Repo, reference: &str, with_token: bool) -> Result<Option<Archive>> {
    agent.check_deadline()?;
    let url = archive_url(agent, repo, reference);
    let timeout = match agent.deadline {
        Some(d) => {
            let remaining = d.saturating_duration_since(std::time::Instant::now());
            // Leave time for the per-file fallback when the stream stalls.
            if remaining < std::time::Duration::from_secs(5) {
                bail!("too little of the automatic time budget left for an archive download");
            }
            remaining
                .saturating_sub(std::time::Duration::from_secs(15))
                .max(std::time::Duration::from_secs(1))
        }
        None => std::time::Duration::from_secs(600),
    };
    let mut req = agent.get(&url).config().timeout_global(Some(timeout)).build();
    if with_token {
        if let Some(h) = agent.source_header(&url, false) {
            req = req.header("Authorization", &h);
        }
    }
    let res = req.call()?;
    match res.status().as_u16() {
        200 => {}
        404 => return Ok(None),
        s @ (301 | 302 | 307 | 308) => bail!("{}/{} moved (HTTP {s}); update the package's repository URL", repo.owner, repo.name),
        s @ (403 | 429) => bail!("codeload.github.com refused {}/{} (HTTP {s})", repo.owner, repo.name),
        s => bail!("codeload.github.com HTTP {s} for {}/{}@{reference}", repo.owner, repo.name),
    }
    Ok(Some(Archive::new(res.into_body().into_reader(), agent.max_archive)))
}

/// Most uncompressed bytes read from one archive (a gzip bomb stops here).
const MAX_INFLATED: u64 = 2 << 30;

/// Counts uncompressed bytes and stops the stream past `MAX_INFLATED`.
struct Inflated<R> {
    inner: R,
    seen: u64,
}
impl<R: Read> Read for Inflated<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.seen += n as u64;
        if self.seen > MAX_INFLATED {
            return Err(std::io::Error::other("archive expands past 2 GiB"));
        }
        Ok(n)
    }
}

/// Stream a `.tar.gz`: for every regular file (leading `<repo>-<ref>/` folder
/// stripped) `keep(path, size)` decides whether to read it. Files over
/// `MAX_FILE` are never read. Kept files stop accumulating past `cap` bytes.
pub(super) fn read_tar(body: impl Read, cap: u64, mut keep: impl FnMut(&str, u64) -> bool) -> Result<Vec<(String, Vec<u8>)>> {
    let inflated = Inflated {
        inner: flate2::read::GzDecoder::new(body),
        seen: 0,
    };
    let mut archive = tar::Archive::new(inflated);
    let mut out = Vec::new();
    let mut total = 0u64;
    for entry in archive.entries().context("not a gzip tar archive")? {
        let entry = entry.context("corrupt or truncated archive")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let size = entry.header().size().unwrap_or(0);
        let raw = entry.path().context("archive path")?.to_string_lossy().replace('\\', "/");
        if raw.starts_with('/') {
            continue;
        }
        let Some((_, path)) = raw.split_once('/') else { continue };
        if path.is_empty() || path.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
            continue;
        }
        let path = path.to_string();
        if !keep(&path, size) || size > MAX_FILE || total.saturating_add(size) > cap {
            continue;
        }
        let mut buf = Vec::with_capacity(size as usize);
        entry.take(MAX_FILE + 1).read_to_end(&mut buf)?;
        total += buf.len() as u64;
        out.push((path, buf));
        if out.len() >= MAX_FILES * 4 {
            break;
        }
    }
    Ok(out)
}

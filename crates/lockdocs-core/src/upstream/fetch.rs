//! Coordinate release and docs-site fetches and publish complete cache entries.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::locate::Source;
use crate::Dep;

use super::cache_entry::{cached, dir, enrich_cached};
use super::client::{agent, Client};
use super::docs_sites::site_step;
use super::git_refs::{list_refs, pick_release, tag_candidates, tag_prefixes};
use super::package_docs::{codeload_docs, rest_docs};
use super::repository::repo_of_with;
use super::{Manifest, Repo, FORMAT};

/// Download the docs folders of `dep`'s repository at the pinned version's tag.
pub fn fetch(dep: &Dep, src: &Source) -> Result<Manifest> {
    fetch_with(dep, src, false, false)
}

/// Explicit fetch. With `private` (lockdocs Pro) it also reads repositories on
/// other git hosts and private repositories, with the user's own credentials.
pub fn fetch_private(dep: &Dep, src: &Source, private: bool) -> Result<Manifest> {
    fetch_with(dep, src, false, private)
}

/// Anonymous, bounded first-use enrichment; never substitutes a major docs site.
pub fn fetch_automatic(dep: &Dep, src: &Source) -> Result<Manifest> {
    fetch_with(dep, src, true, false)
}

struct FetchGuard {
    lock: PathBuf,
    staging: PathBuf,
}
impl Drop for FetchGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.staging);
        let _ = std::fs::remove_file(&self.lock);
    }
}

fn publish(out: &Path, target: &Path, m: &Manifest) -> Result<()> {
    std::fs::write(out.join(".lockdocs-upstream.json"), serde_json::to_string(m)?)?;
    let backup = target.with_file_name(format!(
        "{}.previous-{}",
        target.file_name().context("cache name")?.to_string_lossy(),
        std::process::id()
    ));
    let had_prior = target.exists();
    if had_prior {
        if backup.exists() {
            bail!("previous cache backup already exists; refusing to overwrite it");
        }
        std::fs::rename(target, &backup)?;
    }
    if let Err(e) = std::fs::rename(out, target) {
        if had_prior {
            let _ = std::fs::rename(&backup, target);
        }
        return Err(e.into());
    }
    if had_prior {
        let _ = std::fs::remove_dir_all(backup);
    }
    Ok(())
}

fn fetch_with(dep: &Dep, src: &Source, automatic: bool, private: bool) -> Result<Manifest> {
    crate::fetch::require_registry_origin(dep)?;
    enrich_cached(cached(dep), !automatic, || fetch_attempt(dep, src, automatic, private))
}

/// The repository exists nowhere this client can see (missing, or private and
/// not readable with what it holds).
#[derive(Debug)]
struct RepoUnavailable(String);
impl std::fmt::Display for RepoUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RepoUnavailable {}

fn fetch_attempt(dep: &Dep, src: &Source, automatic: bool, private: bool) -> Result<Manifest> {
    let use_private = private && !automatic;
    let repo = repo_of_with(dep, src, !automatic).context("no GitHub repository in the package metadata")?;
    if let Some(origin) = &repo.host {
        if !use_private {
            return Err(crate::private::PrivateRequired {
                package: dep.id(),
                host: origin.host.clone(),
            }
            .into());
        }
    }
    let target = dir(dep);
    let parent = target.parent().context("upstream cache parent")?;
    std::fs::create_dir_all(parent)?;
    let stem = target.file_name().context("upstream cache name")?.to_string_lossy();
    let lock = target.with_file_name(format!("{stem}.fetch-lock"));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .context("upstream fetch already in progress (or an interrupted fetch left its lock; use cache clean)")?;
    let out = target.with_file_name(format!("{stem}.staging-{}", std::process::id()));
    let _guard = FetchGuard { lock, staging: out.clone() };
    std::fs::create_dir_all(&out)?;
    let git_host = repo.host.clone().map(crate::private::git::Host::of);
    let env = crate::private::Env::process();
    // Another host's credential may come from `git credential fill`. github.com
    // reads the token variables first and asks the helpers only if that fails.
    let creds_for = |fill: bool| {
        use_private.then(|| {
            let h = git_host
                .clone()
                .unwrap_or_else(|| crate::private::git::Host::of(crate::private::Url::parse("https://github.com/").unwrap().origin()));
            (git_host.clone(), crate::private::git::credentials(&h, &env, fill))
        })
    };
    let agent = agent(automatic, creds_for(git_host.is_some()));
    let m = match build(&agent, dep, &repo, automatic, &out) {
        Err(e) if use_private && git_host.is_none() && agent.host_creds.0.is_empty() && e.downcast_ref::<RepoUnavailable>().is_some() => {
            let retry = creds_for(true);
            if retry.as_ref().is_none_or(|(_, c)| c.0.is_empty()) {
                return Err(e);
            }
            let _ = std::fs::remove_dir_all(&out);
            std::fs::create_dir_all(&out)?;
            build(&self::agent(automatic, retry), dep, &repo, automatic, &out)?
        }
        other => other?,
    };
    publish(&out, &target, &m)?;
    Ok(m)
}

/// Download the docs of `dep`'s release (and docs site) into `out`; the caller publishes.
pub(super) fn build(agent: &Client, dep: &Dep, repo: &Repo, automatic: bool, out: &Path) -> Result<Manifest> {
    let label = format!(
        "{}/{}/{}",
        repo.host
            .as_ref()
            .map_or_else(|| "github.com".to_string(), |o| o.to_string().replace("https://", "")),
        repo.owner,
        repo.name
    );
    let candidates = tag_candidates(dep);
    let refs = list_refs(agent, repo, &tag_prefixes(&candidates))?;
    let repo_unavailable = refs.is_none();
    let release = refs.and_then(|refs| pick_release(&candidates, &refs));
    let Some((tag, commit)) = release else {
        let mut m = Manifest {
            format: FORMAT,
            repo: label,
            tag: None,
            commit: None,
            docs_sites_checked: !automatic,
            files: 0,
            bytes: 0,
            note: Some(format!("no git tag found for {} (tried {})", dep.version, candidates.join(", "))),
            site: None,
            pages: Vec::new(),
            via: None,
        };
        let (found, skipped) = site_step(agent, dep, repo, automatic, out)?;
        if let Some(site) = found.filter(|s| s.files > 0) {
            m.files = site.files;
            m.bytes = site.bytes;
            m.site = Some(site.label);
            m.pages = site.pages;
            m.note = None;
        }
        if let Some(n) = skipped {
            m.note = Some(m.note.take().map_or(n.clone(), |old| format!("{old}; {n}")));
        }
        // The repository is gone, renamed or private. A docs site may still serve the
        // docs; fail clearly only when nothing was found anywhere.
        if repo_unavailable && !automatic && m.files == 0 {
            if agent.credentialed_sources {
                return Err(RepoUnavailable(format!("{}: the repository does not exist or the configured credential cannot read it", m.repo)).into());
            }
            return Err(RepoUnavailable(format!("{}: private repository: not supported (or the repository does not exist)", m.repo)).into());
        }
        return Ok(m);
    };
    // One archive of the exact release commit; only the docs are kept. When the
    // archive is over the cap or codeload fails, this repository alone is read
    // file by file through the REST API instead.
    let (docs_n, docs_bytes, via) = match codeload_docs(agent, dep, repo, &tag, &commit, out) {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_dir_all(out);
            std::fs::create_dir_all(out)?;
            if agent.git_host.is_some() {
                return Err(e.context(format!("archive download failed; the per-file fallback is GitHub-only ({})", agent.hosts.git)));
            }
            let (n, bytes) = rest_docs(agent, dep, repo, &commit, out).with_context(|| format!("codeload path failed ({e:#}); per-file fallback failed"))?;
            (n, bytes, "rest")
        }
    };
    // A separate docs-site repository, when the package keeps its docs there.
    let mut site = None;
    let mut site_n = 0usize;
    let mut site_bytes = 0u64;
    let mut pages = Vec::new();
    let (found, skipped) = site_step(agent, dep, repo, automatic, out)?;
    if let Some(s) = found {
        (site_n, site_bytes, pages) = (s.files, s.bytes, s.pages);
        if site_n > 0 {
            site = Some(s.label);
        }
    }
    let m = Manifest {
        format: FORMAT,
        repo: label,
        tag: Some(tag),
        commit: Some(commit),
        docs_sites_checked: !automatic,
        files: docs_n + site_n,
        bytes: docs_bytes + site_bytes,
        note: match (docs_n == 0 && site_n == 0, skipped) {
            (true, None) => Some("no docs folder at that tag".into()),
            (true, Some(n)) => Some(format!("no docs folder at that tag; {n}")),
            (false, n) => n,
        },
        site,
        pages,
        via: Some(via.into()),
    };
    Ok(m)
}

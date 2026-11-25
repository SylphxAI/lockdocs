//! Fetch docs for the pinned major and classify files from the site.

use std::path::Path;

use anyhow::{Context, Result};

use super::catalog::{docs_site, latest_major, next_major_date};
use crate::upstream::archive::{codeload, read_tar};
use crate::upstream::client::{api, enc, Client};
use crate::upstream::git_refs::list_refs;
use crate::upstream::github_files::{download, tree, write_file};
use crate::upstream::package_docs::doc_file;
use crate::upstream::{Repo, MAX_BYTES, MAX_FILE};
use crate::Dep;

/// Files read from an archive: path and content.
type Docs = Vec<(String, Vec<u8>)>;

/// What a docs site contributed: label, files written, bytes, page paths.
pub(in crate::upstream) struct SiteDocs {
    pub(in crate::upstream) label: String,
    pub(in crate::upstream) files: usize,
    pub(in crate::upstream) bytes: u64,
    pub(in crate::upstream) pages: Vec<String>,
}

/// Download the files of the docs site that describe `dep`'s major into
/// `<out>/<site repo name>/`, if any.
pub(in crate::upstream) fn site_docs(agent: &Client, dep: &Dep, pkg_repo: Option<&Repo>, out: &Path) -> Result<Option<super::SiteDocs>> {
    let Some(site) = docs_site(dep) else {
        return Ok(None);
    };
    let major: u64 = dep.version.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    let archive = site.repo == ("reactjs", "legacy.reactjs.org");
    let latest = if archive { Some(major) } else { latest_major(agent, dep)? };
    let repo = Repo {
        owner: site.repo.0.into(),
        name: site.repo.1.into(),
        subdir: None,
        host: None,
    };
    let versioned: Vec<String> = site.versioned.iter().map(|d| d.replace("{major}", &major.to_string())).collect();
    let is_latest = latest.is_some_and(|l| major >= l);
    // Where the current-major folders come from for this version.
    let mut reference = site.branch.to_string();
    let mut how = if archive {
        format!("official archive for major {major}")
    } else {
        String::from("current docs; your major is the latest")
    };
    let mut current = is_latest;
    if !is_latest && latest.is_some() && (!site.current.is_empty() || !site.pages.is_empty()) {
        let branches = [format!("v{major}"), format!("{major}.x")];
        let found = list_refs(agent, &repo, &branches.iter().map(|b| format!("refs/heads/{b}")).collect::<Vec<_>>())?;
        let branch = found.and_then(|refs| branches.iter().find(|b| refs.iter().any(|r| r.name == format!("refs/heads/{b}"))).cloned());
        if let Some(b) = branch {
            how = format!("branch {b} for major {major}");
            reference = b;
            current = true;
        } else if site.before_next_major {
            if let Some(date) = pkg_repo.map(|r| next_major_date(agent, dep, r, major)).transpose()?.flatten() {
                let path = format!(
                    "/repos/{}/{}/commits?sha={}&until={}&per_page=1",
                    repo.owner,
                    repo.name,
                    enc(site.branch),
                    enc(&date)
                );
                if let Some(sha) = api(agent, &path)?.and_then(|v| v.pointer("/0/sha").and_then(|s| s.as_str()).map(String::from)) {
                    reference = sha;
                    how = format!("as of {}, before {} was released", &date[..10.min(date.len())], major + 1);
                    current = true;
                }
            }
        }
    }
    if versioned.is_empty() && !current {
        return Ok(None);
    }
    let docs_dirs: Vec<String> = if current {
        site.current.iter().map(|d| d.to_string()).collect()
    } else {
        Vec::new()
    };
    let page_dirs: Vec<String> = if current {
        site.pages.iter().map(|d| d.to_string()).collect()
    } else {
        Vec::new()
    };
    // Versioned API pages always come from the default branch.
    let versioned_ok = reference == site.branch;
    let keep = |p: &str| super::site_kind(p, major, &versioned, versioned_ok, &docs_dirs, &page_dirs);
    let dst = out.join(&repo.name);
    let streamed = (|| -> Result<Option<Docs>> {
        let Some(body) = codeload(agent, &repo, &reference)? else {
            return Ok(None);
        };
        Ok(Some(read_tar(body.body, MAX_BYTES * 2, |p, _| keep(p).is_some())?))
    })();
    let (mut files, mut bytes, mut pages) = (0usize, 0u64, Vec::new());
    match streamed {
        Ok(None) => return Ok(None),
        Ok(Some(got)) => {
            for (p, data) in got {
                write_file(&dst, &p, &data)?;
                files += 1;
                bytes += data.len() as u64;
                if keep(&p) == Some(true) {
                    pages.push(format!("{}/{p}", repo.name));
                }
            }
        }
        Err(e) => {
            // Over the cap (tailwindcss.com is 190 MB, prisma/docs 860 MB) or codeload
            // failed: read this docs site file by file (one tree request, then raw files).
            let _ = std::fs::remove_dir_all(&dst);
            let Some(items) = tree(agent, &repo, &reference, true).with_context(|| format!("codeload path failed ({e:#}); per-file fallback failed"))? else {
                return Ok(None);
            };
            let wanted: Vec<(String, u64)> = items
                .iter()
                .filter(|i| i.kind == "blob" && i.size <= MAX_FILE)
                .filter_map(|i| keep(&i.path).map(|_| (i.path.clone(), i.size)))
                .collect();
            let ok = download(agent, &repo, &reference, &wanted, &dst).with_context(|| format!("codeload path failed ({e:#}); per-file fallback failed"))?;
            files = ok.len();
            bytes = ok.iter().sum();
            pages = wanted
                .iter()
                .filter(|(p, _)| keep(p) == Some(true))
                .map(|(p, _)| format!("{}/{p}", repo.name))
                .collect();
        }
    }
    pages.sort();
    let short: String = reference.chars().take(if reference.len() == 40 { 7 } else { 40 }).collect();
    let label = format!(
        "github.com/{}/{}@{short} ({})",
        repo.owner,
        repo.name,
        if current { how.as_str() } else { "versioned API pages" }
    );
    Ok(Some(SiteDocs { label, files, bytes, pages }))
}

/// Which docs-site files to keep: `Some(false)` a docs page, `Some(true)` a
/// page written as a JS/TSX component, `None` skip.
pub(in crate::upstream) fn site_kind(
    p: &str,
    major: u64,
    versioned: &[String],
    versioned_ok: bool,
    docs_dirs: &[String],
    page_dirs: &[String],
) -> Option<bool> {
    let under = |dirs: &[String]| dirs.iter().any(|d| p.starts_with(&format!("{d}/")));
    if super::later_major_file(p, major) {
        return None;
    }
    if doc_file(p) && !p.ends_with(".txt") && ((versioned_ok && under(versioned)) || under(docs_dirs)) {
        Some(false)
    } else if super::page_file(p) && under(page_dirs) {
        Some(true)
    } else {
        None
    }
}

/// A page about a later major than the pinned one (`v4-beta.mdx` on the v3
/// branch): it would answer with APIs this version does not have.
pub(in crate::upstream) fn later_major_file(p: &str, major: u64) -> bool {
    let path = p.to_ascii_lowercase();
    path.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w.strip_prefix('v').and_then(|n| n.parse::<u64>().ok()).is_some_and(|n| n > major))
}

/// A docs page written as a JS/TSX component (not layouts or helpers).
pub(in crate::upstream) fn page_file(p: &str) -> bool {
    let name = p.rsplit('/').next().unwrap_or(p);
    [".tsx", ".jsx", ".js"].iter().any(|e| name.ends_with(e))
        && !name.starts_with("layout.")
        && !name.starts_with("index.ts")
        && !p.contains("/@")
        && !p.contains("[")
}

/// The docs site of `dep`, if it has one. An explicit fetch fails on an error;
/// an automatic one keeps the package docs it has and says what was skipped
/// (the cache stays unchecked, so `lockdocs fetch` retries it).
pub(in crate::upstream) fn site_step(agent: &Client, dep: &Dep, repo: &Repo, automatic: bool, out: &Path) -> Result<(Option<SiteDocs>, Option<String>)> {
    if repo.host.is_some() {
        return Ok((None, None));
    }
    match super::site_docs(agent, dep, Some(repo), out) {
        Ok(s) => Ok((s, None)),
        Err(e) if automatic => {
            if let Some(site) = docs_site(dep) {
                let _ = std::fs::remove_dir_all(out.join(site.repo.1));
            }
            Ok((None, Some(format!("docs site skipped: {e:#}"))))
        }
        Err(e) => Err(e.context("docs-site selection failed")),
    }
}

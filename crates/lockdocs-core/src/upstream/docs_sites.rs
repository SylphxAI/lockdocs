//! Select and fetch docs from separate sites that match the pinned major.

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::{Dep, Eco};

use super::archive::{codeload, read_tar};
use super::client::{api, enc, Client};
use super::git_refs::{list_refs, pick_release, tag_candidates, tag_prefixes};
use super::github_files::{download, tree, write_file};
use super::package_docs::doc_file;
use super::{Repo, MAX_BYTES, MAX_FILE};

/// Packages whose docs live in a separate website repository. Paths with
/// `{major}` are versioned and used for any major. The `current` folders
/// describe one major: the latest one on `branch`, an older one on a
/// `v{major}` / `{major}.x` branch or, when `before_next_major` is set, at the
/// last commit before the next major was released.
#[derive(Clone)]
pub(super) struct DocsSite {
    eco: Eco,
    names: &'static [&'static str],
    pub(super) repo: (&'static str, &'static str),
    branch: &'static str,
    pub(super) versioned: &'static [&'static str],
    /// Docs folders; layouts move between majors, so every one that exists is used.
    pub(super) current: &'static [&'static str],
    /// Folders of docs pages written as JS/TSX components (installation steps).
    pub(super) pages: &'static [&'static str],
    /// Older majors may use the commit before the next major's release.
    /// Off for sites that document APIs before they ship (react.dev).
    before_next_major: bool,
}

pub(super) const DOCS_SITES: &[DocsSite] = &[
    DocsSite {
        eco: Eco::Npm,
        names: &["react", "react-dom", "@types/react"],
        repo: ("reactjs", "react.dev"),
        branch: "main",
        versioned: &[],
        current: &["src/content/reference", "src/content/learn"],
        pages: &[],
        before_next_major: false,
    },
    DocsSite {
        eco: Eco::Npm,
        names: &["express", "@types/express"],
        repo: ("expressjs", "expressjs.com"),
        branch: "main",
        versioned: &["src/content/api/{major}x"],
        current: &["src/content/docs/en"],
        pages: &[],
        before_next_major: false,
    },
    DocsSite {
        eco: Eco::Npm,
        names: &["tailwindcss"],
        repo: ("tailwindlabs", "tailwindcss.com"),
        branch: "main",
        versioned: &[],
        current: &["src/docs", "src/pages/docs"],
        pages: &["src/app/(docs)/docs/installation", "src/pages/docs/installation"],
        before_next_major: true,
    },
    DocsSite {
        eco: Eco::Npm,
        names: &["prisma", "@prisma/client"],
        repo: ("prisma", "docs"),
        branch: "main",
        versioned: &[],
        current: &["apps/docs/content/docs/orm", "content/200-orm"],
        pages: &[],
        before_next_major: true,
    },
    DocsSite {
        eco: Eco::Cargo,
        names: &["tokio"],
        repo: ("tokio-rs", "website"),
        branch: "master",
        versioned: &[],
        current: &["content/tokio/tutorial", "content/tokio/topics"],
        pages: &[],
        before_next_major: false,
    },
];

/// The latest stable major of a package, from its registry.
fn latest_major(agent: &Client, dep: &Dep) -> Result<Option<u64>> {
    let name = dep.name.strip_prefix("@types/").unwrap_or(&dep.name);
    let (url, pointer) = match dep.eco {
        Eco::Npm => (format!("{}/{}/latest", agent.hosts.npm, name.replace('/', "%2F")), "/version"),
        Eco::Cargo => (format!("https://crates.io/api/v1/crates/{name}"), "/crate/max_stable_version"),
        Eco::PyPI => (format!("https://pypi.org/pypi/{name}/json"), "/info/version"),
        Eco::Go => return Ok(None),
    };
    agent.check_deadline()?;
    let mut res = agent.get(&url).call().with_context(|| format!("latest-major lookup: {url}"))?;
    let status = res.status().as_u16();
    let mut body = String::new();
    res.body_mut()
        .as_reader()
        .take((16 << 20) + 1)
        .read_to_string(&mut body)
        .context("latest-major response read failed")?;
    if body.len() > 16 << 20 {
        bail!("latest-major response exceeds 16 MB");
    }
    latest_major_response(status, &body, pointer)
}

/// Only a genuine missing registry record is an empty lookup. Network, HTTP,
/// malformed JSON and invalid version responses must leave enrichment retryable.
pub(super) fn latest_major_response(status: u16, body: &str, pointer: &str) -> Result<Option<u64>> {
    if status == 404 {
        return Ok(None);
    }
    if status != 200 {
        bail!("latest-major registry lookup failed: HTTP {status}");
    }
    let v: Value = serde_json::from_str(body).context("invalid latest-major registry JSON")?;
    let version = v.pointer(pointer).and_then(Value::as_str).context("latest-major response missing version")?;
    let major = version
        .split('.')
        .next()
        .context("latest-major response has empty version")?
        .parse()
        .context("latest-major response has invalid version")?;
    Ok(Some(major))
}

/// When the package's next major was released: the commit date of its
/// `{major+1}.0.0` tag in the package repository.
fn next_major_date(agent: &Client, dep: &Dep, repo: &Repo, major: u64) -> Result<Option<String>> {
    let next = Dep {
        version: format!("{}.0.0", major + 1),
        ..dep.clone()
    };
    let candidates = tag_candidates(&next);
    let Some(refs) = list_refs(agent, repo, &tag_prefixes(&candidates))? else {
        return Ok(None);
    };
    let Some((_, sha)) = pick_release(&candidates, &refs) else {
        return Ok(None);
    };
    // The one place that still needs a commit date, which only the REST API gives.
    let Some(v) = api(agent, &format!("/repos/{}/{}/commits/{sha}", repo.owner, repo.name))? else {
        return Ok(None);
    };
    Ok(v.pointer("/commit/committer/date").and_then(|d| d.as_str()).map(String::from))
}

/// Files read from an archive: path and content.
type Docs = Vec<(String, Vec<u8>)>;

/// What a docs site contributed: label, files written, bytes, page paths.
pub(super) struct SiteDocs {
    pub(super) label: String,
    pub(super) files: usize,
    pub(super) bytes: u64,
    pub(super) pages: Vec<String>,
}

/// Resolve the mapped site, including older-major archive destinations. Fetch
/// and failure cleanup must agree on the repository directory they own.
fn docs_site(dep: &Dep) -> Option<DocsSite> {
    let mut site = DOCS_SITES.iter().find(|s| s.eco == dep.eco && s.names.contains(&dep.name.as_str()))?.clone();
    // react.dev includes APIs before release. Use the official frozen legacy
    // site for React 18 and its version archives for 16/17, never a date guess.
    if site.repo == ("reactjs", "react.dev") {
        let archive = match dep.version.split('.').next()? {
            "16" => Some("16.reactjs.org"),
            "17" => Some("17.reactjs.org"),
            "18" => Some("main"),
            _ => None,
        };
        if let Some(branch) = archive {
            site.repo = ("reactjs", "legacy.reactjs.org");
            site.branch = branch;
            site.current = &["content/docs"];
        }
    }
    Some(site)
}

/// Download the files of the docs site that describe `dep`'s major into
/// `<out>/<site repo name>/`, if any.
pub(super) fn site_docs(agent: &Client, dep: &Dep, pkg_repo: Option<&Repo>, out: &Path) -> Result<Option<SiteDocs>> {
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
    let keep = |p: &str| site_kind(p, major, &versioned, versioned_ok, &docs_dirs, &page_dirs);
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
pub(super) fn site_kind(p: &str, major: u64, versioned: &[String], versioned_ok: bool, docs_dirs: &[String], page_dirs: &[String]) -> Option<bool> {
    let under = |dirs: &[String]| dirs.iter().any(|d| p.starts_with(&format!("{d}/")));
    if later_major_file(p, major) {
        return None;
    }
    if doc_file(p) && !p.ends_with(".txt") && ((versioned_ok && under(versioned)) || under(docs_dirs)) {
        Some(false)
    } else if page_file(p) && under(page_dirs) {
        Some(true)
    } else {
        None
    }
}

/// A page about a later major than the pinned one (`v4-beta.mdx` on the v3
/// branch): it would answer with APIs this version does not have.
pub(super) fn later_major_file(p: &str, major: u64) -> bool {
    let path = p.to_ascii_lowercase();
    path.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w.strip_prefix('v').and_then(|n| n.parse::<u64>().ok()).is_some_and(|n| n > major))
}

/// A docs page written as a JS/TSX component (not layouts or helpers).
pub(super) fn page_file(p: &str) -> bool {
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
pub(super) fn site_step(agent: &Client, dep: &Dep, repo: &Repo, automatic: bool, out: &Path) -> Result<(Option<SiteDocs>, Option<String>)> {
    if repo.host.is_some() {
        return Ok((None, None));
    }
    match site_docs(agent, dep, Some(repo), out) {
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

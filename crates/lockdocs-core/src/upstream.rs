//! Upstream docs: many packages ship no documentation (Next.js, Django,
//! FastAPI...). Their repositories do, under the exact version's git tag.
//! This module finds the repository from the package's own metadata, finds
//! the tag for the pinned version, and downloads only the docs folders
//! (Markdown, MDX, reStructuredText) from GitHub into the cache, once.
//! First-use queries fetch public docs at an immutable release commit. Explicit
//! `fetch` also supports major-version docs sites and optional GitHub credentials.

use crate::locate::Source;
use crate::{cache, Dep, Eco};
use anyhow::{bail, Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_FILES: usize = 2500;
const MAX_BYTES: u64 = 40 * 1024 * 1024;
const MAX_FILE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
    /// Package directory inside a monorepo (npm `repository.directory`).
    pub subdir: Option<String>,
}

/// Bump when what `fetch` downloads changes, so `lockdocs fetch` refreshes
/// older copies (a stale copy is still used until then).
pub const FORMAT: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// `FORMAT` when downloaded (0 before it existed).
    #[serde(default)]
    pub format: u32,
    pub repo: String,
    pub tag: Option<String>,
    /// Immutable package-repository commit resolved from the release tag.
    #[serde(default)]
    pub commit: Option<String>,
    /// Explicit enrichment completed a docs-site lookup (including a genuine empty result).
    #[serde(default)]
    pub docs_sites_checked: bool,
    pub files: usize,
    pub bytes: u64,
    /// Why nothing was downloaded, when files == 0.
    pub note: Option<String>,
    /// A separate docs-site repository also included (see `DOCS_SITES`).
    #[serde(default)]
    pub site: Option<String>,
    /// Docs pages written as JS/TSX components, relative to the cache folder.
    #[serde(default)]
    pub pages: Vec<String>,
    /// How the package repository was read: `codeload` (one tarball) or `rest`
    /// (the per-file fallback, after a codeload failure or an over-size archive).
    #[serde(default)]
    pub via: Option<String>,
}

/// Packages whose docs live in a separate website repository. Paths with
/// `{major}` are versioned and used for any major. The `current` folders
/// describe one major: the latest one on `branch`, an older one on a
/// `v{major}` / `{major}.x` branch or, when `before_next_major` is set, at the
/// last commit before the next major was released.
struct DocsSite {
    eco: Eco,
    names: &'static [&'static str],
    repo: (&'static str, &'static str),
    branch: &'static str,
    versioned: &'static [&'static str],
    /// Docs folders; layouts move between majors, so every one that exists is used.
    current: &'static [&'static str],
    /// Folders of docs pages written as JS/TSX components (installation steps).
    pages: &'static [&'static str],
    /// Older majors may use the commit before the next major's release.
    /// Off for sites that document APIs before they ship (react.dev).
    before_next_major: bool,
}

const DOCS_SITES: &[DocsSite] = &[
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
        Eco::Npm => (format!("https://registry.npmjs.org/{}/latest", name.replace('/', "%2F")), "/version"),
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
fn latest_major_response(status: u16, body: &str, pointer: &str) -> Result<Option<u64>> {
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
struct SiteDocs {
    label: String,
    files: usize,
    bytes: u64,
    pages: Vec<String>,
}

/// Download the files of the docs site that describe `dep`'s major into
/// `<out>/<site repo name>/`, if any.
fn site_docs(agent: &Client, dep: &Dep, pkg_repo: Option<&Repo>, out: &Path) -> Result<Option<SiteDocs>> {
    let Some(site) = DOCS_SITES.iter().find(|s| s.eco == dep.eco && s.names.contains(&dep.name.as_str())) else {
        return Ok(None);
    };
    let major: u64 = dep.version.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    let latest = latest_major(agent, dep)?;
    let repo = Repo {
        owner: site.repo.0.into(),
        name: site.repo.1.into(),
        subdir: None,
    };
    let versioned: Vec<String> = site.versioned.iter().map(|d| d.replace("{major}", &major.to_string())).collect();
    let is_latest = latest.is_some_and(|l| major >= l);
    // Where the current-major folders come from for this version.
    let mut reference = site.branch.to_string();
    let mut how = String::from("current docs; your major is the latest");
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
            // Over the cap or codeload failed: read this docs site file by file.
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
fn site_kind(p: &str, major: u64, versioned: &[String], versioned_ok: bool, docs_dirs: &[String], page_dirs: &[String]) -> Option<bool> {
    let under = |dirs: &[String]| dirs.iter().any(|d| p.starts_with(&format!("{d}/")));
    if doc_file(p) && !p.ends_with(".txt") && !later_major_file(p, major) && ((versioned_ok && under(versioned)) || under(docs_dirs)) {
        Some(false)
    } else if page_file(p) && under(page_dirs) {
        Some(true)
    } else {
        None
    }
}

/// A page about a later major than the pinned one (`v4-beta.mdx` on the v3
/// branch): it would answer with APIs this version does not have.
fn later_major_file(p: &str, major: u64) -> bool {
    let name = p.rsplit('/').next().unwrap_or(p).to_ascii_lowercase();
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w.strip_prefix('v').and_then(|n| n.parse::<u64>().ok()).is_some_and(|n| n > major))
}

/// A docs page written as a JS/TSX component (not layouts or helpers).
fn page_file(p: &str) -> bool {
    let name = p.rsplit('/').next().unwrap_or(p);
    [".tsx", ".jsx", ".js"].iter().any(|e| name.ends_with(e))
        && !name.starts_with("layout.")
        && !name.starts_with("index.ts")
        && !p.contains("/@")
        && !p.contains("[")
}

/// Parse a GitHub URL or shorthand into owner/name.
pub fn parse_github(url: &str) -> Option<(String, String)> {
    let u = url.trim();
    let rest = if let Some(r) = u.strip_prefix("github:") {
        r
    } else if let Some(i) = u.find("github.com") {
        u[i + "github.com".len()..].trim_start_matches([':', '/'])
    } else if !u.contains(':') && u.matches('/').count() == 1 && !u.starts_with('.') {
        u // npm "owner/repo" shorthand
    } else {
        return None;
    };
    let mut it = rest.split(['/', '#', '?']).filter(|s| !s.is_empty());
    let owner = it.next()?.to_string();
    let name = it.next()?.trim_end_matches(".git").to_string();
    let safe = |s: &str| !s.is_empty() && s != "." && s != ".." && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    if !safe(&owner) || !safe(&name) {
        return None;
    }
    Some((owner, name))
}

/// The package's source repository, from its own metadata.
pub fn repo_of(dep: &Dep, src: &Source) -> Option<Repo> {
    match dep.eco {
        Eco::Npm => {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(src.dir.join("package.json")).ok()?).ok()?;
            let r = v.get("repository")?;
            let (url, dir) = match r {
                Value::String(s) => (s.clone(), None),
                _ => (
                    r.get("url")?.as_str()?.to_string(),
                    r.get("directory").and_then(|d| d.as_str()).map(String::from),
                ),
            };
            let (owner, name) = parse_github(&url)?;
            Some(Repo { owner, name, subdir: dir })
        }
        Eco::PyPI => {
            let meta = std::fs::read_to_string(src.metadata.as_ref()?).ok()?;
            let mut best: Option<(u8, (String, String))> = None;
            for line in meta.lines().take_while(|l| !l.trim().is_empty()) {
                let (key, val) = line.split_once(':')?;
                let (rank, url) = match key.trim() {
                    "Project-URL" => {
                        let (label, url) = val.split_once(',')?;
                        let l = label.trim().to_ascii_lowercase();
                        let rank = if ["source", "repository", "source code", "code", "github"].contains(&l.as_str()) {
                            3
                        } else {
                            1
                        };
                        (rank, url.trim().to_string())
                    }
                    "Home-page" => (2, val.trim().to_string()),
                    _ => continue,
                };
                if let Some(gh) = parse_github(&url) {
                    if best.as_ref().is_none_or(|b| rank > b.0) {
                        best = Some((rank, gh));
                    }
                }
            }
            let (_, (owner, name)) = best?;
            Some(Repo { owner, name, subdir: None })
        }
        Eco::Cargo => {
            let t: toml::Table = toml::from_str(&std::fs::read_to_string(src.dir.join("Cargo.toml")).ok()?).ok()?;
            let url = t.get("package")?.get("repository")?.as_str()?;
            let (owner, name) = parse_github(url)?;
            Some(Repo { owner, name, subdir: None })
        }
        Eco::Go => {
            let rest = dep.name.strip_prefix("github.com/")?;
            let mut it = rest.split('/');
            let owner = it.next()?.to_string();
            let name = it.next()?.to_string();
            let sub: Vec<&str> = it.filter(|s| !(s.starts_with('v') && s[1..].chars().all(|c| c.is_ascii_digit()))).collect();
            Some(Repo {
                owner,
                name,
                subdir: if sub.is_empty() { None } else { Some(sub.join("/")) },
            })
        }
    }
}

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

fn token() -> Option<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
}

/// The GitHub hosts requests go to (replaced by a local server in tests).
#[derive(Clone)]
struct Hosts {
    api: String,
    git: String,
    codeload: String,
    raw: String,
}

impl Hosts {
    fn github() -> Self {
        Hosts {
            api: "https://api.github.com".into(),
            git: "https://github.com".into(),
            codeload: "https://codeload.github.com".into(),
            raw: "https://raw.githubusercontent.com".into(),
        }
    }
}

struct Client {
    agent: ureq::Agent,
    authenticated: bool,
    /// Future Pro "private sources" hook: when true, the token is also sent to
    /// the git and codeload hosts. Always false today (see `source_auth`).
    credentialed_sources: bool,
    deadline: Option<std::time::Instant>,
    hosts: Hosts,
    /// Largest compressed archive streamed from one repository.
    max_archive: u64,
}

impl std::ops::Deref for Client {
    type Target = ureq::Agent;
    fn deref(&self) -> &Self::Target {
        &self.agent
    }
}

impl Client {
    fn check_deadline(&self) -> Result<()> {
        if self.deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            bail!("automatic upstream fetch time budget exhausted");
        }
        Ok(())
    }
}

fn agent(automatic: bool) -> Client {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(if automatic { 10 } else { 60 })))
        .max_redirects(if automatic { 0 } else { 10 })
        .http_status_as_error(false)
        // Renamed repositories answer with a redirect; the REST token follows it on the same host only.
        .redirect_auth_headers(ureq::config::RedirectAuthHeaders::SameHost)
        .user_agent(concat!("lockdocs/", env!("CARGO_PKG_VERSION"), " (+https://github.com/SylphxAI/lockdocs)"))
        .build()
        .into();
    Client {
        agent,
        authenticated: !automatic,
        credentialed_sources: false,
        deadline: automatic.then(|| std::time::Instant::now() + std::time::Duration::from_secs(45)),
        hosts: Hosts::github(),
        max_archive: if automatic { MAX_ARCHIVE_AUTOMATIC } else { MAX_ARCHIVE },
    }
}

impl Client {
    /// The token for the git smart-HTTP, codeload and raw hosts. Free lockdocs
    /// sends no credential to any of them; only the future Pro "private
    /// sources" path sets `credentialed_sources`. The REST API alone gets the
    /// token on explicit fetch (see `api`).
    fn source_auth(&self) -> Option<String> {
        (self.authenticated && self.credentialed_sources).then(token).flatten()
    }
}

/// GitHub API GET; Ok(None) on 404.
fn api(agent: &Client, path: &str) -> Result<Option<Value>> {
    agent.check_deadline()?;
    let mut req = agent.get(&format!("{}{path}", agent.hosts.api)).header("Accept", "application/vnd.github+json");
    if let Some(t) = agent.authenticated.then(token).flatten() {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }
    let mut res = req.call()?;
    let status = res.status().as_u16();
    if status == 404 || status == 422 || status == 409 {
        return Ok(None);
    }
    let mut body = String::new();
    res.body_mut().as_reader().take(64 << 20).read_to_string(&mut body)?;
    if status == 403 || status == 429 {
        bail!("GitHub API rate limit (HTTP {status}); automatic fetch is anonymous; explicit `lockdocs fetch` can use GITHUB_TOKEN");
    }
    if status >= 300 {
        bail!("GitHub API HTTP {status} for {path}");
    }
    Ok(Some(serde_json::from_str(&body)?))
}

/// A ref advertised by `git ls-remote`: full name, object id, and the commit
/// an annotated tag peels to.
#[derive(Debug, Clone, PartialEq)]
struct GitRef {
    name: String,
    oid: String,
    peeled: Option<String>,
}

fn pkt_line(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(format!("{:04x}", s.len() + 4).as_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Body of a protocol-v2 `ls-refs` command limited to `prefixes`.
fn ls_refs_request(prefixes: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    pkt_line(&mut out, "command=ls-refs\n");
    out.extend_from_slice(b"0001");
    pkt_line(&mut out, "peel\n");
    for p in prefixes {
        pkt_line(&mut out, &format!("ref-prefix {p}\n"));
    }
    out.extend_from_slice(b"0000");
    out
}

/// Parse an `ls-refs` response (pkt-lines: `<oid> <name>[ peeled:<oid>]`).
fn parse_ls_refs(mut b: &[u8]) -> Result<Vec<GitRef>> {
    let mut refs = Vec::new();
    while b.len() >= 4 {
        let len = std::str::from_utf8(&b[..4])
            .ok()
            .and_then(|h| usize::from_str_radix(h, 16).ok())
            .context("malformed git response")?;
        if len < 4 {
            // flush (0000), delimiter (0001), response end (0002)
            b = &b[4..];
            if len == 0 {
                break;
            }
            continue;
        }
        if b.len() < len {
            bail!("truncated git response");
        }
        let line = String::from_utf8_lossy(&b[4..len]);
        b = &b[len..];
        let line = line.trim_end_matches('\n');
        if let Some(e) = line.strip_prefix("ERR ") {
            bail!("git server error: {e}");
        }
        let mut parts = line.split(' ');
        let (Some(oid), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        let peeled = parts.find_map(|a| a.strip_prefix("peeled:")).map(String::from);
        refs.push(GitRef {
            name: name.to_string(),
            oid: oid.to_string(),
            peeled,
        });
    }
    Ok(refs)
}

fn is_object_id(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn tag_prefixes(candidates: &[String]) -> Vec<String> {
    candidates.iter().map(|t| format!("refs/tags/{t}")).collect()
}

/// The first candidate tag that exists exactly, with the immutable commit it
/// points to (annotated tags are peeled by the server).
fn pick_release(candidates: &[String], refs: &[GitRef]) -> Option<(String, String)> {
    candidates.iter().find_map(|t| {
        let name = format!("refs/tags/{t}");
        let r = refs.iter().find(|r| r.name == name)?;
        let sha = r.peeled.as_deref().unwrap_or(&r.oid);
        is_object_id(sha).then(|| (t.clone(), sha.to_string()))
    })
}

/// Basic credentials for GitHub's git endpoints (`x-access-token:<token>`).
fn basic(token: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = format!("x-access-token:{token}");
    let mut out = String::new();
    for c in input.as_bytes().chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    format!("Basic {out}")
}

/// Refs under `prefixes` of a repository over the git smart-HTTP protocol (what
/// `git ls-remote` does). It is not governed by the REST API quota. Ok(None)
/// when the repository does not exist or is private.
fn list_refs(agent: &Client, repo: &Repo, prefixes: &[String]) -> Result<Option<Vec<GitRef>>> {
    agent.check_deadline()?;
    let url = format!("{}/{}/{}.git/git-upload-pack", agent.hosts.git, repo.owner, repo.name);
    let mut req = agent
        .post(&url)
        .header("Content-Type", "application/x-git-upload-pack-request")
        .header("Accept", "application/x-git-upload-pack-result")
        .header("Git-Protocol", "version=2");
    let sent_token = match agent.source_auth() {
        Some(t) => {
            req = req.header("Authorization", &basic(&t));
            true
        }
        None => false,
    };
    let mut res = req.send(&ls_refs_request(prefixes)[..])?;
    let status = res.status().as_u16();
    if status == 401 && sent_token {
        bail!(
            "GitHub rejected GITHUB_TOKEN/GH_TOKEN for {}/{} (HTTP 401); refresh or unset it",
            repo.owner,
            repo.name
        );
    }
    if status == 404 || status == 401 {
        return Ok(None);
    }
    if status >= 300 {
        bail!("git refs for {}/{}: HTTP {status}", repo.owner, repo.name);
    }
    let mut body = Vec::new();
    res.body_mut().as_reader().take(64 << 20).read_to_end(&mut body)?;
    Ok(Some(parse_ls_refs(&body)?))
}

/// Compressed bytes downloaded from codeload by this process (for reporting).
static DOWNLOADED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Compressed archive bytes downloaded by this process so far.
pub fn downloaded_bytes() -> u64 {
    DOWNLOADED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Largest compressed archive streamed from one repository by default. A
/// bigger one is not downloaded whole: the repository falls back to per-file
/// requests (see the measurements in the PR for the choice).
const MAX_ARCHIVE: u64 = 150 << 20;

/// The cap for the automatic (anonymous, 45 s budget) mode.
const MAX_ARCHIVE_AUTOMATIC: u64 = 64 << 20;

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
struct Archive {
    body: Box<dyn Read>,
    exceeded: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
fn codeload(agent: &Client, repo: &Repo, reference: &str) -> Result<Option<Archive>> {
    let sent_token = agent.source_auth().is_some();
    match codeload_once(agent, repo, reference, sent_token)? {
        // Only reachable on the future credentialed path: a token codeload does not
        // accept falls back to an anonymous request, which public repositories answer.
        None if sent_token => codeload_once(agent, repo, reference, false),
        other => Ok(other),
    }
}

fn codeload_once(agent: &Client, repo: &Repo, reference: &str, with_token: bool) -> Result<Option<Archive>> {
    agent.check_deadline()?;
    let url = format!("{}/{}/{}/tar.gz/{}", agent.hosts.codeload, repo.owner, repo.name, enc_path(reference));
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
        if let Some(t) = agent.source_auth() {
            req = req.header("Authorization", &format!("Bearer {t}"));
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
fn read_tar(body: impl Read, cap: u64, mut keep: impl FnMut(&str, u64) -> bool) -> Result<Vec<(String, Vec<u8>)>> {
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

fn write_file(root: &Path, p: &str, data: &[u8]) -> Result<()> {
    let Some(rel) = crate::fetch::safe_rel(Path::new(p), false) else {
        bail!("unsafe path {p}")
    };
    let dst = root.join(rel);
    if let Some(d) = dst.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(dst, data)?;
    Ok(())
}

/// Which files of a package repository to keep. A docs root is a `docs`-like
/// folder at the top, or one or two levels inside wrapper folders (monorepos
/// keep them there); README/CHANGELOG-like files sit at the top or in the
/// package's own folder.
struct PkgScan {
    short: String,
    sub_base: String,
    subdir: Option<String>,
    roots: std::collections::BTreeSet<String>,
    langs: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// (full path, docs root it sits in, path inside that root, is a README-like file)
    cands: Vec<(String, Option<String>, String, bool)>,
}

impl PkgScan {
    fn new(dep: &Dep, repo: &Repo) -> Self {
        PkgScan {
            short: dep.name.rsplit('/').next().unwrap_or(&dep.name).to_ascii_lowercase(),
            sub_base: repo.subdir.as_deref().and_then(|s| s.rsplit('/').next()).unwrap_or("").to_ascii_lowercase(),
            subdir: repo.subdir.clone(),
            roots: Default::default(),
            langs: Default::default(),
            cands: Vec::new(),
        }
    }

    /// Number of leading folders that form the docs root of `dirs`, if any.
    fn root_len(&self, dirs: &[&str]) -> Option<usize> {
        for (i, d) in dirs.iter().take(3).enumerate() {
            let name = d.to_ascii_lowercase();
            if DOC_ROOTS.contains(&name.as_str()) {
                return Some(i + 1);
            }
            // Only wanted folders are opened to look further down.
            let full = dirs[..=i].join("/");
            let wanted = WRAPPERS.contains(&name.as_str())
                || (i >= 1 && (name == self.short || name == self.sub_base || name.contains("docs")))
                || self.subdir.as_deref().is_some_and(|s| s == full || s.starts_with(&format!("{full}/")));
            if i >= 2 || !wanted {
                return None;
            }
        }
        None
    }

    /// Look at one archive path; true when its content may be wanted.
    fn observe(&mut self, path: &str, size: u64) -> bool {
        let comps: Vec<&str> = path.split('/').collect();
        let (file, dirs) = comps.split_last().expect("non-empty path");
        let root = self.root_len(dirs).map(|n| dirs[..n].join("/"));
        let mut rel_in_root = None;
        if let Some(r) = &root {
            self.roots.insert(r.clone());
            let rel = &comps[r.split('/').count()..];
            if rel.len() > 1 {
                self.langs.entry(r.clone()).or_default().insert(rel[0].to_string());
            }
            rel_in_root = Some(rel.join("/"));
        }
        let upper = file.to_ascii_uppercase();
        let near = dirs.is_empty() || self.subdir.as_deref() == Some(dirs.join("/").as_str());
        let readme = near
            && doc_file(file)
            && ["README", "CHANGELOG", "CHANGES", "HISTORY", "MIGRAT", "UPGRAD", "RELEASE"]
                .iter()
                .any(|p| upper.starts_with(p));
        let in_root = rel_in_root.as_deref().is_some_and(doc_file);
        let wanted = size <= MAX_FILE && (readme || in_root);
        if wanted {
            self.cands
                .push((path.to_string(), root.filter(|_| in_root), rel_in_root.unwrap_or_default(), readme));
        }
        wanted
    }

    /// Apply the root, language and size limits to what was read.
    fn select(self, mut data: std::collections::HashMap<String, Vec<u8>>) -> Vec<(String, Vec<u8>)> {
        let chosen: Vec<&String> = self.roots.iter().take(4).collect();
        let mut keep: Vec<String> = Vec::new();
        for (full, root, rel, readme) in &self.cands {
            let in_chosen = root.as_ref().is_some_and(|r| chosen.contains(&r));
            if !in_chosen && !readme {
                continue;
            }
            if in_chosen && !readme {
                let langs = self.langs.get(root.as_ref().expect("root"));
                let only_en = langs.is_some_and(|l| l.iter().filter(|x| is_lang(x)).count() >= 2 && l.contains("en"));
                let first = rel.split('/').next().unwrap_or("");
                if only_en && rel.contains('/') && is_lang(first) && first != "en" {
                    continue;
                }
            }
            keep.push(full.clone());
        }
        keep.sort();
        keep.dedup();
        let mut total = 0u64;
        let mut out = Vec::new();
        for p in keep {
            let Some(d) = data.remove(&p) else { continue };
            if total.saturating_add(d.len() as u64) > MAX_BYTES {
                continue;
            }
            total += d.len() as u64;
            out.push((p, d));
            if out.len() >= MAX_FILES {
                break;
            }
        }
        out
    }
}

struct Item {
    path: String,
    kind: String,
    sha: String,
    size: u64,
}

fn tree(agent: &Client, repo: &Repo, sha_or_ref: &str, recursive: bool) -> Result<Option<Vec<Item>>> {
    let q = if recursive { "?recursive=1" } else { "" };
    let Some(v) = api(agent, &format!("/repos/{}/{}/git/trees/{}{q}", repo.owner, repo.name, enc(sha_or_ref)))? else {
        return Ok(None);
    };
    let items = v
        .get("tree")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .map(|i| Item {
                    path: i.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                    kind: i.get("type").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                    sha: i.get("sha").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                    size: i.get("size").and_then(|p| p.as_u64()).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(items))
}

fn download(agent: &Client, repo: &Repo, reference: &str, files: &[(String, u64)], out: &Path) -> Result<Vec<u64>> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(16).build()?;
    let results: Vec<Result<u64>> = pool.install(|| {
        files
            .par_iter()
            .map(|(p, _)| {
                let url = format!("{}/{}/{}/{}/{}", agent.hosts.raw, repo.owner, repo.name, enc(reference), enc_path(p));
                agent.check_deadline()?;
                let mut res = agent.get(&url).call()?;
                if res.status().as_u16() != 200 {
                    bail!("HTTP {} for {p}", res.status());
                }
                let mut buf = Vec::new();
                res.body_mut().as_reader().take(MAX_FILE + 1).read_to_end(&mut buf)?;
                if buf.len() as u64 > MAX_FILE {
                    bail!("upstream file too large: {p}");
                }
                let Some(rel) = crate::fetch::safe_rel(Path::new(p), false) else {
                    bail!("unsafe path {p}")
                };
                let dst = out.join(rel);
                if let Some(d) = dst.parent() {
                    std::fs::create_dir_all(d)?;
                }
                std::fs::write(dst, &buf)?;
                Ok(buf.len() as u64)
            })
            .collect()
    });
    download_results(results)
}

fn download_results(results: Vec<Result<u64>>) -> Result<Vec<u64>> {
    let failed = results.iter().filter(|r| r.is_err()).count();
    if failed > 0 {
        let first = results.iter().find_map(|r| r.as_ref().err()).expect("failed download");
        bail!("{failed}/{} upstream files failed to download: {first:#}", results.len());
    }
    results.into_iter().collect()
}

/// The per-file fallback for one repository: GitHub's REST tree walk plus raw
/// file requests (subject to the REST quota). Returns files written and bytes.
fn rest_docs(agent: &Client, dep: &Dep, repo: &Repo, commit: &str, out: &Path) -> Result<(usize, u64)> {
    let root = tree(agent, repo, commit, false)?.context("release commit tree not found")?;
    let short = dep.name.rsplit('/').next().unwrap_or(&dep.name).to_ascii_lowercase();
    let sub_base = repo.subdir.as_deref().and_then(|s| s.rsplit('/').next()).unwrap_or("").to_ascii_lowercase();
    let mut roots: Vec<(String, String)> = Vec::new(); // (path, tree sha)
    let mut files: Vec<(String, u64)> = Vec::new();
    let mut calls = 0;
    let mut frontier: Vec<(String, Vec<Item>, usize)> = vec![(String::new(), root, 0)];
    while let Some((prefix, items, depth)) = frontier.pop() {
        for it in items {
            let name = it.path.to_ascii_lowercase();
            let full = if prefix.is_empty() {
                it.path.clone()
            } else {
                format!("{prefix}/{}", it.path)
            };
            if it.kind == "blob" {
                // Root-level (or package-level) READMEs and changelogs.
                let upper = it.path.to_ascii_uppercase();
                let near = depth == 0 || repo.subdir.as_deref() == Some(prefix.as_str());
                if near
                    && doc_file(&it.path)
                    && ["README", "CHANGELOG", "CHANGES", "HISTORY", "MIGRAT", "UPGRAD", "RELEASE"]
                        .iter()
                        .any(|p| upper.starts_with(p))
                {
                    files.push((full, it.size));
                }
                continue;
            }
            if it.kind != "tree" {
                continue;
            }
            if DOC_ROOTS.contains(&name.as_str()) {
                roots.push((full, it.sha));
            } else if depth < 2 && calls < 10 {
                let wanted = WRAPPERS.contains(&name.as_str())
                    || (depth >= 1 && (name == short || name == sub_base || name.contains("docs")))
                    || repo.subdir.as_deref().is_some_and(|s| s == full || s.starts_with(&format!("{full}/")));
                if wanted {
                    calls += 1;
                    if let Some(children) = tree(agent, repo, &it.sha, false)? {
                        frontier.push((full, children, depth + 1));
                    }
                }
            }
        }
    }
    for (path, sha) in roots.iter().take(4) {
        let Some(items) = tree(agent, repo, sha, true)? else { continue };
        // Keep English when the docs are split by language.
        let langs: Vec<&str> = items
            .iter()
            .filter(|i| i.kind == "tree" && !i.path.contains('/') && is_lang(&i.path))
            .map(|i| i.path.as_str())
            .collect();
        let only_en = langs.len() >= 2 && langs.contains(&"en");
        for i in &items {
            if i.kind != "blob" || !doc_file(&i.path) || i.size > MAX_FILE {
                continue;
            }
            if only_en {
                let first = i.path.split('/').next().unwrap_or("");
                if is_lang(first) && first != "en" {
                    continue;
                }
            }
            files.push((format!("{path}/{}", i.path), i.size));
        }
    }
    files.sort();
    files.dedup();
    let mut total = 0u64;
    files.retain(|(_, s)| {
        if *s > MAX_FILE || total.saturating_add(*s) > MAX_BYTES {
            return false;
        }
        total += s;
        true
    });
    files.truncate(MAX_FILES);
    let ok = download(agent, repo, commit, &files, out)?;
    Ok((ok.len(), ok.iter().sum()))
}

/// Download one repository snapshot and keep its docs (see `PkgScan`).
fn extract_pkg_docs(body: impl Read, dep: &Dep, repo: &Repo) -> Result<Vec<(String, Vec<u8>)>> {
    let mut scan = PkgScan::new(dep, repo);
    let data = read_tar(body, MAX_BYTES * 2, |p, size| scan.observe(p, size))?;
    Ok(scan.select(data.into_iter().collect()))
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn enc_path(p: &str) -> String {
    p.split('/').map(enc).collect::<Vec<_>>().join("/")
}

/// Tag names projects use for a release.
pub fn tag_candidates(dep: &Dep) -> Vec<String> {
    let v = dep.version.trim_start_matches('v');
    let short = dep.name.rsplit('/').next().unwrap_or(&dep.name);
    let mut c = vec![
        format!("v{v}"),
        v.to_string(),
        format!("{}@{v}", dep.name),
        format!("{short}@{v}"),
        format!("{short}-v{v}"),
        format!("{short}-{v}"),
        format!("rel_{}", v.replace('.', "_")),
        format!("release-{v}"),
    ];
    if dep.eco == Eco::Go {
        c.insert(0, dep.version.clone());
    }
    c.dedup();
    c
}

const DOC_ROOTS: &[&str] = &["docs", "doc", "documentation", "guide", "guides", "docs_src"];
const WRAPPERS: &[&str] = &["website", "site", "www", "apps", "packages", "content", "src"];

fn doc_file(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    let example = (l.contains("example") || l.contains("snippet") || l.starts_with("docs_src/"))
        && [".py", ".ts", ".tsx", ".js", ".jsx", ".rs", ".go"].iter().any(|e| l.ends_with(e));
    // Django and others write reStructuredText in .txt files under docs/.
    let ext_ok = example || [".md", ".mdx", ".rst", ".markdown", ".mdoc", ".txt"].iter().any(|e| l.ends_with(e));
    let skip = [
        "/blog/",
        "/node_modules/",
        "/i18n/",
        "/_snippets/",
        "/images/",
        "/public/",
        "/.github/",
        "/test/",
        "/tests/",
    ]
    .iter()
    .any(|s| format!("/{l}").contains(s));
    ext_ok && !skip
}

fn is_lang(s: &str) -> bool {
    (s.len() == 2 && s.chars().all(|c| c.is_ascii_lowercase())) || (s.len() == 5 && s.as_bytes()[2] == b'-')
}

/// Download the docs folders of `dep`'s repository at the pinned version's tag.
pub fn fetch(dep: &Dep, src: &Source) -> Result<Manifest> {
    fetch_with(dep, src, false)
}

/// Anonymous, bounded first-use enrichment; never substitutes a major docs site.
pub fn fetch_automatic(dep: &Dep, src: &Source) -> Result<Manifest> {
    fetch_with(dep, src, true)
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

fn fetch_with(dep: &Dep, src: &Source, automatic: bool) -> Result<Manifest> {
    crate::fetch::require_registry_origin(dep)?;
    enrich_cached(cached(dep), !automatic, || fetch_attempt(dep, src, automatic))
}

fn enrich_cached(prior: Option<(PathBuf, Manifest)>, explicit: bool, attempt: impl FnOnce() -> Result<Manifest>) -> Result<Manifest> {
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

fn fetch_attempt(dep: &Dep, src: &Source, automatic: bool) -> Result<Manifest> {
    let repo = repo_of(dep, src).context("no GitHub repository in the package metadata")?;
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
    let agent = agent(automatic);
    let m = build(&agent, dep, &repo, automatic, &out)?;
    publish(&out, &target, &m)?;
    Ok(m)
}

/// Download the docs of `dep`'s release (and docs site) into `out`; the caller publishes.
fn build(agent: &Client, dep: &Dep, repo: &Repo, automatic: bool, out: &Path) -> Result<Manifest> {
    let label = format!("github.com/{}/{}", repo.owner, repo.name);
    let candidates = tag_candidates(dep);
    let refs = list_refs(agent, repo, &tag_prefixes(&candidates))?;
    if refs.is_none() && !automatic {
        bail!("{label}: private repository: not supported (or the repository does not exist)");
    }
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
        match if automatic { Ok(None) } else { site_docs(agent, dep, Some(repo), out) } {
            Ok(Some(site)) => {
                if site.files > 0 {
                    m.files = site.files;
                    m.bytes = site.bytes;
                    m.site = Some(site.label);
                    m.pages = site.pages;
                    m.note = None;
                }
            }
            Ok(None) => {}
            Err(e) => return Err(e.context("docs-site selection failed")),
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
            let (n, bytes) = rest_docs(agent, dep, repo, &commit, out).with_context(|| format!("codeload path failed ({e:#}); per-file fallback failed"))?;
            (n, bytes, "rest")
        }
    };
    // A separate docs-site repository, when the package keeps its docs there.
    let mut site = None;
    let mut site_n = 0usize;
    let mut site_bytes = 0u64;
    let mut pages = Vec::new();
    match if automatic { Ok(None) } else { site_docs(agent, dep, Some(repo), out) } {
        Ok(Some(s)) => {
            (site_n, site_bytes, pages) = (s.files, s.bytes, s.pages);
            if site_n > 0 {
                site = Some(s.label);
            }
        }
        Ok(None) => {}
        Err(e) => return Err(e.context("docs-site selection failed")),
    }
    let m = Manifest {
        format: FORMAT,
        repo: label,
        tag: Some(tag),
        commit: Some(commit),
        docs_sites_checked: !automatic,
        files: docs_n + site_n,
        bytes: docs_bytes + site_bytes,
        note: if docs_n == 0 && site_n == 0 {
            Some("no docs folder at that tag".into())
        } else {
            None
        },
        site,
        pages,
        via: Some(via.into()),
    };
    Ok(m)
}

/// Stream the release archive and write its docs: (files, bytes, "codeload").
/// Errors when the archive is missing, over the cap, or unreadable.
fn codeload_docs(agent: &Client, dep: &Dep, repo: &Repo, tag: &str, commit: &str, out: &Path) -> Result<(usize, u64, &'static str)> {
    let Some(body) = codeload(agent, repo, commit)? else {
        bail!("tag {tag} points to commit {commit}, but codeload.github.com has no archive for it (HTTP 404)");
    };
    let read = extract_pkg_docs(body.body, dep, repo);
    let docs = match read {
        Ok(d) => d,
        Err(_) if body.exceeded.load(std::sync::atomic::Ordering::Relaxed) => {
            bail!("archive is over the {} MB download cap", agent.max_archive >> 20)
        }
        Err(e) => return Err(e.context(format!("reading the archive of {} at {tag}", repo.name))),
    };
    let mut bytes = 0u64;
    for (p, d) in &docs {
        write_file(out, p, d)?;
        bytes += d.len() as u64;
    }
    Ok((docs.len(), bytes, "codeload"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        Manifest {
            format: FORMAT,
            repo: "github.com/o/r".into(),
            tag: Some("v1.2.3".into()),
            commit: Some("0123456789abcdef0123456789abcdef01234567".into()),
            docs_sites_checked: false,
            files: 1,
            bytes: 10,
            note: None,
            site: None,
            pages: Vec::new(),
            via: None,
        }
    }

    fn dep(name: &str, version: &str) -> Dep {
        Dep {
            eco: Eco::Npm,
            name: name.into(),
            version: version.into(),
            direct: true,
            from: "x".into(),
        }
    }

    fn repo(subdir: Option<&str>) -> Repo {
        Repo {
            owner: "o".into(),
            name: "r".into(),
            subdir: subdir.map(String::from),
        }
    }

    /// A small `<repo>-<ref>/...` codeload-shaped archive built in memory.
    fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        for (p, data) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_entry_type(tar::EntryType::Regular);
            b.append_data(&mut h, format!("r-abc/{p}"), *data).unwrap();
        }
        // A symlink and a directory must never be extracted.
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        b.append_link(&mut h, "r-abc/docs/link.md", "/etc/passwd").unwrap();
        let tar = b.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        gz.finish().unwrap()
    }

    fn names(v: &[(String, Vec<u8>)]) -> Vec<&str> {
        v.iter().map(|(p, _)| p.as_str()).collect()
    }

    #[test]
    fn ls_refs_round_trip_and_release_pick() {
        let c1 = "1111111111111111111111111111111111111111";
        let c2 = "2222222222222222222222222222222222222222";
        let tagobj = "3333333333333333333333333333333333333333";
        let req = String::from_utf8(ls_refs_request(&["refs/tags/v1.2.3".into(), "refs/tags/1.2.3".into()])).unwrap();
        assert!(req.starts_with("0014command=ls-refs\n00010009peel\n"));
        assert!(req.contains("0020ref-prefix refs/tags/v1.2.3\n") && req.ends_with("0000"));
        let mut resp = String::new();
        for l in [
            format!("{tagobj} refs/tags/v1.2.3 peeled:{c1}\n"),
            format!("{c2} refs/tags/v1.2.3-beta.1\n"),
            format!("{c2} refs/tags/1.2.3\n"),
        ] {
            resp.push_str(&format!("{:04x}{l}", l.len() + 4));
        }
        resp.push_str("0000");
        let refs = parse_ls_refs(resp.as_bytes()).unwrap();
        assert_eq!(refs.len(), 3);
        let cands = tag_candidates(&dep("pkg", "1.2.3"));
        // Annotated tag: the peeled commit, never the tag object; prefix matches are not exact.
        assert_eq!(pick_release(&cands, &refs), Some(("v1.2.3".into(), c1.into())));
        // Candidate order decides, a lightweight tag is its own commit.
        assert_eq!(pick_release(&["1.2.3".into(), "v1.2.3".into()], &refs), Some(("1.2.3".into(), c2.into())));
        // Only a prefix match (a prerelease) is not a release.
        assert_eq!(pick_release(&["v1.2.3".into()], &refs[1..2]), None);
        assert_eq!(pick_release(&cands, &[]), None);
        // Never a branch of the same name, and never a malformed id.
        let branch = [GitRef {
            name: "refs/heads/v1.2.3".into(),
            oid: c1.into(),
            peeled: None,
        }];
        assert_eq!(pick_release(&cands, &branch), None);
        let bad = [GitRef {
            name: "refs/tags/v1.2.3".into(),
            oid: "xyz".into(),
            peeled: None,
        }];
        assert_eq!(pick_release(&cands, &bad), None);
        assert!(parse_ls_refs(b"0010ERR denied\n0000").is_err());
        assert!(parse_ls_refs(b"00ffshort").is_err());
        assert!(parse_ls_refs(b"zzzz").is_err());
    }

    /// A local stand-in for GitHub: git smart-HTTP, codeload, REST and raw.
    /// `api_blocked` answers every REST request 403, like an exhausted quota.
    struct Fake {
        base: String,
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Codeload {
        Ok,
        Fail,
        /// 404 when a token is sent, the archive when anonymous.
        MissingWithAuth,
        /// The git endpoint answers 401 when a token is sent.
        BadToken,
        /// A private repository: anonymous git answers 401 and codeload 404.
        Private,
        /// Answers after a pause longer than a short deadline.
        Slow,
    }

    const SHA: &str = "1111111111111111111111111111111111111111";

    fn fake(codeload: Codeload, api_blocked: bool, tag: Option<&str>) -> Fake {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
        let log = seen.clone();
        let tag = tag.map(String::from);
        let archive = tarball(&[("README.md", b"# readme"), ("docs/guide.md", b"# guide"), ("src/lib.rs", b"fn x() {}")]);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut c) = stream else { return };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = c.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let line = head.lines().next().unwrap_or("").to_string();
                let path = line.split(' ').nth(1).unwrap_or("").to_string();
                let authed = head.to_ascii_lowercase().contains("\r\nauthorization:");
                log.lock().unwrap().push(format!(
                    "{} {}{}",
                    line.split(' ').next().unwrap_or(""),
                    path,
                    if authed { " AUTH" } else { "" }
                ));
                let json = |v: serde_json::Value| (200, "application/json", v.to_string().into_bytes());
                let (status, ctype, body): (u16, &str, Vec<u8>) =
                    if path.ends_with("/git-upload-pack") && (codeload == Codeload::Private || (authed && codeload == Codeload::BadToken)) {
                        (401, "text/plain", Vec::new())
                    } else if path.ends_with("/git-upload-pack") {
                        // Drain the request body (pkt-lines end with 0000).
                        let mut body = buf.split(|_| false).next().unwrap().to_vec();
                        while !body.ends_with(b"0000") {
                            let n = c.read(&mut chunk).unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            body.extend_from_slice(&chunk[..n]);
                        }
                        let mut resp = String::new();
                        if let Some(t) = &tag {
                            let l = format!("{SHA} refs/tags/{t}\n");
                            resp.push_str(&format!("{:04x}{l}", l.len() + 4));
                        }
                        resp.push_str("0000");
                        (200, "application/x-git-upload-pack-result", resp.into_bytes())
                    } else if path.starts_with("/o/r/tar.gz/") {
                        match codeload {
                            Codeload::Ok => (200, "application/gzip", archive.clone()),
                            Codeload::Private => (404, "text/plain", Vec::new()),
                            Codeload::Fail => (500, "text/plain", b"boom".to_vec()),
                            Codeload::MissingWithAuth if authed => (404, "text/plain", Vec::new()),
                            Codeload::MissingWithAuth | Codeload::BadToken => (200, "application/gzip", archive.clone()),
                            Codeload::Slow => {
                                std::thread::sleep(std::time::Duration::from_secs(8));
                                (200, "application/gzip", archive.clone())
                            }
                        }
                    } else if path.starts_with("/repos/") {
                        if api_blocked {
                            (403, "application/json", br#"{"message":"API rate limit exceeded"}"#.to_vec())
                        } else if path.contains("/git/trees/docs-sha") {
                            json(serde_json::json!({"tree":[{"path":"guide.md","type":"blob","sha":"b2","size":7}]}))
                        } else if path.contains("/git/trees/") {
                            json(serde_json::json!({"tree":[
                            {"path":"README.md","type":"blob","sha":"b1","size":8},
                            {"path":"docs","type":"tree","sha":"docs-sha","size":0},
                            {"path":"src","type":"tree","sha":"src-sha","size":0}]}))
                        } else {
                            (404, "application/json", b"{}".to_vec())
                        }
                    } else if path.starts_with("/raw/o/r/") {
                        (200, "text/markdown", b"# raw".to_vec())
                    } else {
                        (404, "text/plain", Vec::new())
                    };
                let _ = write!(
                    c,
                    "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = c.write_all(&body);
            }
        });
        Fake { base, seen }
    }

    fn client(f: &Fake, max_archive: u64) -> Client {
        let agent = ureq::Agent::config_builder().http_status_as_error(false).proxy(None).build().into();
        Client {
            agent,
            authenticated: false,
            credentialed_sources: false,
            deadline: None,
            hosts: Hosts {
                api: f.base.clone(),
                git: f.base.clone(),
                codeload: f.base.clone(),
                raw: format!("{}/raw", f.base),
            },
            max_archive,
        }
    }

    fn authed_client(f: &Fake) -> Client {
        let mut c = client(f, MAX_ARCHIVE);
        c.authenticated = true;
        c
    }

    /// A client with the future credentialed-sources hook switched on.
    fn credentialed_client(f: &Fake) -> Client {
        let mut c = authed_client(f);
        c.credentialed_sources = true;
        c
    }

    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_token<T>(f: impl FnOnce() -> T) -> T {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("GITHUB_TOKEN", "test-token");
        std::env::remove_var("GH_TOKEN");
        let r = f();
        std::env::remove_var("GITHUB_TOKEN");
        r
    }

    fn authed_requests(f: &Fake) -> usize {
        f.seen.lock().unwrap().iter().filter(|l| l.ends_with(" AUTH")).count()
    }

    fn run_build(f: &Fake, max_archive: u64) -> (Result<Manifest>, tempfile::TempDir) {
        let out = tempfile::tempdir().unwrap();
        let r = build(&client(f, max_archive), &dep("pkg", "1.2.3"), &repo(None), false, out.path());
        (r, out)
    }

    fn requests(f: &Fake, prefix: &str) -> usize {
        f.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.split(' ').nth(1).unwrap_or("").starts_with(prefix))
            .count()
    }

    #[test]
    fn fetch_with_rest_api_blocked_succeeds_through_codeload() {
        let f = fake(Codeload::Ok, true, Some("v1.2.3"));
        let (m, out) = run_build(&f, MAX_ARCHIVE);
        let m = m.unwrap();
        assert_eq!(
            (m.tag.as_deref(), m.commit.as_deref(), m.via.as_deref()),
            (Some("v1.2.3"), Some(SHA), Some("codeload"))
        );
        assert_eq!(m.files, 2);
        assert_eq!(std::fs::read_to_string(out.path().join("docs/guide.md")).unwrap(), "# guide");
        assert!(!out.path().join("src").exists());
        // Neither the REST API nor raw file requests were made.
        assert_eq!(requests(&f, "/repos/") + requests(&f, "/raw/"), 0);
        assert_eq!(requests(&f, &format!("/o/r/tar.gz/{SHA}")), 1);
    }

    #[test]
    fn over_cap_archive_falls_back_to_per_file_for_that_repository() {
        let f = fake(Codeload::Ok, false, Some("v1.2.3"));
        let (m, out) = run_build(&f, 64);
        let m = m.unwrap();
        assert_eq!(m.via.as_deref(), Some("rest"));
        assert!(m.files >= 2 && out.path().join("README.md").is_file() && out.path().join("docs/guide.md").is_file());
        assert!(requests(&f, "/repos/") > 0 && requests(&f, "/raw/") > 0);
    }

    #[test]
    fn codeload_failure_falls_back_and_both_failing_reports_both() {
        let f = fake(Codeload::Fail, false, Some("v1.2.3"));
        assert_eq!(run_build(&f, MAX_ARCHIVE).0.unwrap().via.as_deref(), Some("rest"));
        let f = fake(Codeload::Fail, true, Some("v1.2.3"));
        let e = format!("{:#}", run_build(&f, MAX_ARCHIVE).0.unwrap_err());
        assert!(e.contains("codeload") && e.contains("fallback failed"), "{e}");
    }

    #[test]
    fn missing_tag_is_a_clear_note_and_downloads_nothing() {
        let f = fake(Codeload::Ok, true, None);
        let m = run_build(&f, MAX_ARCHIVE).0.unwrap();
        let note = m.note.unwrap();
        assert!(
            note.contains("no git tag found for 1.2.3") && note.contains("v1.2.3") && note.contains("1.2.3,"),
            "{note}"
        );
        assert_eq!((m.tag, m.files), (None, 0));
        assert_eq!(requests(&f, "/o/r/tar.gz/") + requests(&f, "/repos/"), 0);
    }

    #[test]
    fn credentialed_hook_rejected_token_is_an_error_but_anonymous_401_is_not() {
        let f = fake(Codeload::BadToken, false, Some("v1.2.3"));
        let e = with_token(|| {
            format!(
                "{:#}",
                list_refs(&credentialed_client(&f), &repo(None), &["refs/tags/v1.2.3".into()]).unwrap_err()
            )
        });
        assert!(e.contains("rejected GITHUB_TOKEN") && e.contains("401"), "{e}");
        // Anonymous requests never send the header, so the same server answers normally.
        assert!(list_refs(&client(&f, MAX_ARCHIVE), &repo(None), &["refs/tags/v1.2.3".into()])
            .unwrap()
            .is_some());
    }

    #[test]
    fn credentialed_hook_codeload_404_retries_anonymously() {
        let f = fake(Codeload::MissingWithAuth, false, Some("v1.2.3"));
        let got = with_token(|| codeload(&credentialed_client(&f), &repo(None), SHA).unwrap());
        assert!(got.is_some());
        let seen = f.seen.lock().unwrap().clone();
        let tar: Vec<_> = seen.iter().filter(|l| l.contains("/tar.gz/")).collect();
        assert_eq!(tar.len(), 2);
        assert!(tar[0].ends_with(" AUTH") && !tar[1].ends_with(" AUTH"), "{tar:?}");
    }

    #[test]
    fn slow_codeload_under_a_short_deadline_falls_back_to_rest() {
        let f = fake(Codeload::Slow, false, Some("v1.2.3"));
        let mut c = client(&f, MAX_ARCHIVE);
        // 17 s left: the stream gets 2 s, the rest of the budget serves the fallback.
        c.deadline = Some(std::time::Instant::now() + std::time::Duration::from_secs(17));
        let out = tempfile::tempdir().unwrap();
        let m = build(&c, &dep("pkg", "1.2.3"), &repo(None), true, out.path()).unwrap();
        assert_eq!(m.via.as_deref(), Some("rest"));
    }

    #[test]
    fn automatic_mode_sends_no_authorization_header() {
        let f = fake(Codeload::Ok, false, Some("v1.2.3"));
        let a = with_token(|| {
            let a = agent(true);
            assert_eq!(a.max_archive, MAX_ARCHIVE_AUTOMATIC);
            assert_eq!(agent(false).max_archive, MAX_ARCHIVE);
            let mut c = client(&f, MAX_ARCHIVE);
            c.authenticated = a.authenticated;
            let out = tempfile::tempdir().unwrap();
            build(&c, &dep("pkg", "1.2.3"), &repo(None), true, out.path()).unwrap();
            a
        });
        assert!(!a.authenticated);
        assert!(requests(&f, "/o/r/tar.gz/") == 1);
        assert_eq!(authed_requests(&f), 0);
    }

    /// Paths that received an Authorization header.
    fn authed_paths(f: &Fake) -> Vec<String> {
        f.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.ends_with(" AUTH"))
            .map(|l| l.split(' ').nth(1).unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn explicit_mode_sends_no_token_to_git_codeload_or_raw() {
        // Archive path: git and codeload only.
        let f = fake(Codeload::Ok, false, Some("v1.2.3"));
        with_token(|| {
            let out = tempfile::tempdir().unwrap();
            build(&authed_client(&f), &dep("pkg", "1.2.3"), &repo(None), false, out.path()).unwrap();
        });
        assert!(requests(&f, "/o/r/tar.gz/") == 1 && requests(&f, "/o/r.git/") >= 1);
        assert_eq!(authed_paths(&f), Vec::<String>::new());
        // Per-file path: git, REST and raw. Only REST carries the token, as in 0.4.0.
        let f = fake(Codeload::Fail, false, Some("v1.2.3"));
        with_token(|| {
            let out = tempfile::tempdir().unwrap();
            build(&authed_client(&f), &dep("pkg", "1.2.3"), &repo(None), false, out.path()).unwrap();
        });
        assert!(requests(&f, "/raw/") >= 1 && requests(&f, "/repos/") >= 1);
        let authed = authed_paths(&f);
        assert!(!authed.is_empty() && authed.iter().all(|p| p.starts_with("/repos/")), "{authed:?}");
    }

    #[test]
    fn public_repository_fetches_anonymously_without_a_token() {
        let f = fake(Codeload::Ok, false, Some("v1.2.3"));
        let out = tempfile::tempdir().unwrap();
        let m = build(&authed_client(&f), &dep("pkg", "1.2.3"), &repo(None), false, out.path()).unwrap();
        assert_eq!(m.via.as_deref(), Some("codeload"));
        assert_eq!(authed_requests(&f), 0);
    }

    #[test]
    fn private_repository_fails_clearly_even_with_a_token() {
        let f = fake(Codeload::Private, false, Some("v1.2.3"));
        let e = with_token(|| {
            let out = tempfile::tempdir().unwrap();
            format!(
                "{:#}",
                build(&authed_client(&f), &dep("pkg", "1.2.3"), &repo(None), false, out.path()).unwrap_err()
            )
        });
        assert!(e.contains("private repository: not supported"), "{e}");
        assert!(!e.contains("Pro") && !e.contains("coming"), "{e}");
        assert_eq!(authed_requests(&f), 0);
        assert_eq!(requests(&f, "/o/r/tar.gz/"), 0);
    }

    #[test]
    fn basic_auth_header_is_base64() {
        assert_eq!(basic("a"), "Basic eC1hY2Nlc3MtdG9rZW46YQ==");
        assert_eq!(basic("ab"), "Basic eC1hY2Nlc3MtdG9rZW46YWI=");
        assert_eq!(basic("abc"), "Basic eC1hY2Nlc3MtdG9rZW46YWJj");
    }

    #[test]
    fn tar_extraction_keeps_only_docs_paths() {
        let big = vec![b'x'; (MAX_FILE + 1) as usize];
        let tgz = tarball(&[
            ("README.md", b"# readme"),
            ("CHANGELOG.md", b"# changes"),
            ("LICENSE", b"mit"),
            ("src/lib.rs", b"fn main() {}"),
            ("docs/index.md", b"# docs"),
            ("docs/guide/a.mdx", b"a"),
            ("docs/img.png", b"png"),
            ("docs/blog/post.md", b"blog"),
            ("docs/huge.md", &big),
            ("packages/pkg/docs/b.md", b"b"),
            ("packages/other/docs/c.md", b"c"), // another package: not opened
            ("node_modules/x/docs/n.md", b"n"),
            ("src/deep/er/est/docs/z.md", b"z"),
        ]);
        let got = extract_pkg_docs(&tgz[..], &dep("pkg", "1.0.0"), &repo(None)).unwrap();
        assert_eq!(
            names(&got),
            vec!["CHANGELOG.md", "README.md", "docs/guide/a.mdx", "docs/index.md", "packages/pkg/docs/b.md",]
        );
        assert_eq!(got.iter().find(|(p, _)| p == "README.md").unwrap().1, b"# readme");
        // The symlink never appears, and nothing outside docs roots is read.
        assert!(!names(&got).iter().any(|p| p.contains("link") || p.starts_with("src/")));
    }

    #[test]
    fn tar_extraction_prefers_english_and_package_readme() {
        let tgz = tarball(&[
            ("docs/en/a.md", b"a"),
            ("docs/fr/a.md", b"fr"),
            ("docs/de/a.md", b"de"),
            ("docs/top.md", b"t"),
            ("README.md", b"root"),
            ("packages/pkg/README.md", b"pkg"),
            ("packages/pkg/src/x.md", b"no"),
            ("packages/other/README.md", b"other"),
        ]);
        let got = extract_pkg_docs(&tgz[..], &dep("pkg", "1.0.0"), &repo(Some("packages/pkg"))).unwrap();
        assert_eq!(names(&got), vec!["README.md", "docs/en/a.md", "docs/top.md", "packages/pkg/README.md"]);
    }

    #[test]
    fn tar_extraction_rejects_unsafe_and_corrupt_archives() {
        // A path that escapes the folder is skipped, not written anywhere.
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_entry_type(tar::EntryType::Regular);
        h.as_old_mut().name[..16].copy_from_slice(b"r-abc/../evil.md");
        h.set_cksum();
        b.append(&h, &b"x"[..]).unwrap();
        let tar = b.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        let tgz = gz.finish().unwrap();
        assert!(read_tar(&tgz[..], 1 << 20, |_, _| true).unwrap().is_empty());
        assert!(write_file(Path::new("/nonexistent-root"), "../x.md", b"x").is_err());
        // Truncated or non-archive bodies are errors, never "no docs".
        let ok = tarball(&[("docs/a.md", &[b'a'; 4096])]);
        assert!(extract_pkg_docs(&ok[..ok.len() / 2], &dep("pkg", "1.0.0"), &repo(None)).is_err());
        assert!(extract_pkg_docs(&b"<html>not found</html>"[..], &dep("pkg", "1.0.0"), &repo(None)).is_err());
    }

    #[test]
    fn hardlinks_and_absolute_paths_are_skipped() {
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Link);
        h.set_size(0);
        b.append_link(&mut h, "r-abc/docs/hard.md", "r-abc/README.md").unwrap();
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_mode(0o644);
        h.set_entry_type(tar::EntryType::Regular);
        let mut raw = [0u8; 100];
        raw[..11].copy_from_slice(b"/abs/doc.md");
        h.as_old_mut().name = raw;
        h.set_cksum();
        b.append(&h, &b"x"[..]).unwrap();
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_mode(0o644);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, "r-abc/docs/ok.md", &b"y"[..]).unwrap();
        let tar = b.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        let tgz = gz.finish().unwrap();
        let got = read_tar(&tgz[..], 1000, |_, _| true).unwrap();
        assert_eq!(names(&got), vec!["docs/ok.md"]);
    }

    #[test]
    fn tar_extraction_stops_at_the_size_cap() {
        let tgz = tarball(&[("docs/a.md", &[b'a'; 600]), ("docs/b.md", &[b'b'; 600])]);
        let got = read_tar(&tgz[..], 1000, |_, _| true).unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn docs_site_files_follow_the_major_rules() {
        let (docs, pages): (Vec<String>, Vec<String>) = (vec!["src/content/docs/en".into()], vec!["src/app/installation".into()]);
        let versioned = vec!["src/content/api/4x".to_string()];
        let k = |p: &str, ok: bool| site_kind(p, 4, &versioned, ok, &docs, &pages);
        assert_eq!(k("src/content/docs/en/a.md", false), Some(false));
        assert_eq!(k("src/content/docs/en/a.txt", false), None);
        assert_eq!(k("src/content/docs/en/v5-beta.md", false), None);
        assert_eq!(k("src/content/api/4x/req.md", true), Some(false));
        assert_eq!(k("src/content/api/4x/req.md", false), None);
        assert_eq!(k("src/app/installation/page.tsx", false), Some(true));
        assert_eq!(k("src/app/installation/layout.tsx", false), None);
        assert_eq!(k("README.md", true), None);
    }

    #[test]
    fn latest_major_failures_remain_retryable_and_explicit_retry_adds_site() {
        assert_eq!(latest_major_response(404, "not found", "/version").unwrap(), None);
        assert_eq!(latest_major_response(200, r#"{"version":"2.3.4"}"#, "/version").unwrap(), Some(2));
        for (status, body) in [
            (503, "unavailable"),
            (429, "limited"),
            (200, "not json"),
            (200, "{}"),
            (200, r#"{"version":"invalid"}"#),
        ] {
            assert!(latest_major_response(status, body, "/version").is_err());
        }
        let prior = manifest();
        let failed = enrich_cached(Some((PathBuf::new(), prior.clone())), true, || {
            latest_major_response(503, "unavailable", "/version")?;
            unreachable!("failed latest lookup cannot mark docs-site enrichment complete")
        })
        .unwrap();
        assert_eq!(failed.files, prior.files);
        assert!(!failed.docs_sites_checked);
        assert!(failed.note.as_deref().unwrap().contains("HTTP 503"));
        let retried = enrich_cached(Some((PathBuf::new(), failed)), true, || {
            assert_eq!(latest_major_response(200, r#"{"version":"2.3.4"}"#, "/version")?, Some(2));
            let mut enriched = prior;
            enriched.docs_sites_checked = true;
            enriched.site = Some("github.com/o/site@fixed (major 2)".into());
            enriched.files += 1;
            Ok(enriched)
        })
        .unwrap();
        assert!(retried.docs_sites_checked && retried.site.is_some());
        assert_eq!(retried.files, 2);
        assert!(retried.note.is_none());
    }

    #[test]
    fn legacy_candidate_commit_cache_cannot_be_trusted_online() {
        let mut legacy = manifest();
        legacy.format = 3;
        legacy.docs_sites_checked = true;
        assert!(needs_refresh(&legacy, false));
        assert!(needs_refresh(&legacy, true));
        let rejected = enrich_cached(Some((PathBuf::new(), legacy)), false, || Err(anyhow::anyhow!("exact tag revalidation failed")));
        assert!(rejected.is_err());
    }

    #[test]
    fn format_4_cache_is_trusted_automatically_and_refreshed_on_fetch() {
        let mut v4 = manifest();
        v4.format = 4;
        v4.docs_sites_checked = true;
        assert!(!needs_refresh(&v4, false));
        assert!(needs_refresh(&v4, true));
        let kept = enrich_cached(Some((PathBuf::new(), v4)), false, || Err(anyhow::anyhow!("offline"))).unwrap();
        assert_eq!(kept.files, 1);
    }

    #[test]
    fn enrichment_policy_upgrades_once_and_preserves_opted_in_sites() {
        let automatic = manifest();
        assert!(needs_refresh(&automatic, true));
        assert!(!needs_refresh(&automatic, false));
        let mut explicit = automatic.clone();
        explicit.docs_sites_checked = true;
        explicit.site = Some("github.com/o/site@resolved (major-version docs)".into());
        explicit.files = 2;
        let upgraded = enrich_cached(Some((PathBuf::new(), automatic)), true, || Ok(explicit.clone())).unwrap();
        assert_eq!(upgraded.files, 2);
        for policy in [false, true] {
            let reused = enrich_cached(Some((PathBuf::new(), upgraded.clone())), policy, || panic!("needless network refresh")).unwrap();
            assert_eq!(reused.site, explicit.site);
        }
    }

    #[test]
    fn failed_downloads_are_not_empty_docs_or_published_partial_success() {
        let old = manifest();
        let preserved = enrich_cached(Some((PathBuf::new(), old.clone())), true, || {
            Err(anyhow::anyhow!("reading the archive: corrupt or truncated archive"))
        })
        .unwrap();
        assert_eq!(preserved.commit, old.commit);
        assert_eq!(preserved.files, old.files);
        assert!(!preserved.docs_sites_checked);
        assert!(preserved.note.unwrap().contains("previous complete cache retained"));
        assert!(enrich_cached(None, true, || Err(anyhow::anyhow!("download failed"))).is_err());
    }

    #[test]
    fn git_origins_are_rejected_before_repository_or_tag_requests() {
        let source = Source {
            dir: PathBuf::new(),
            files: None,
            metadata: None,
            version: "1.2.3".into(),
            label: "checkout".into(),
            fetched: false,
        };
        for (eco, from) in [(Eco::Cargo, "Cargo.lock (git)"), (Eco::PyPI, "uv.lock (git)")] {
            let dep = Dep {
                eco,
                name: "git-fixture".into(),
                version: "1.2.3".into(),
                direct: true,
                from: from.into(),
            };
            for result in [
                fetch(&dep, &source),
                fetch_automatic(&dep, &source),
                crate::fetch::fetch(&dep).map(|_| manifest()),
            ] {
                assert!(result.unwrap_err().to_string().contains("git dependency"));
            }
        }
    }

    #[test]
    fn automatic_client_is_anonymous_and_bounded() {
        let mut client = agent(true);
        assert!(!client.authenticated);
        assert!(client.deadline.is_some());
        client.deadline = Some(std::time::Instant::now());
        assert!(client.check_deadline().is_err());
        assert!(agent(false).authenticated);
    }

    #[test]
    fn github_urls() {
        let g = |s: &str| parse_github(s).map(|(o, n)| format!("{o}/{n}"));
        assert_eq!(g("git+https://github.com/vercel/next.js.git").as_deref(), Some("vercel/next.js"));
        assert_eq!(g("https://github.com/pydantic/pydantic").as_deref(), Some("pydantic/pydantic"));
        assert_eq!(g("git@github.com:colinhacks/zod.git").as_deref(), Some("colinhacks/zod"));
        assert_eq!(g("github:tokio-rs/axum").as_deref(), Some("tokio-rs/axum"));
        assert_eq!(g("expressjs/express").as_deref(), Some("expressjs/express"));
        assert_eq!(g("https://gitlab.com/x/y"), None);
        assert_eq!(g("github:../repo"), None);
        assert_eq!(g("github:owner/.."), None);
        assert_eq!(g("github:owner/repo%2f.."), None);
        let d = Dep {
            eco: Eco::PyPI,
            name: "sqlalchemy".into(),
            version: "2.0.36".into(),
            direct: true,
            from: "x".into(),
        };
        let t = tag_candidates(&d);
        assert_eq!(t[0], "v2.0.36");
        assert!(t.contains(&"rel_2_0_36".to_string()));
        assert!(doc_file("docs/01-app/page.mdx") && !doc_file("docs/blog/x.md") && !doc_file("docs/img.png"));
        assert!(later_major_file("src/pages/docs/v4-beta.mdx", 3) && !later_major_file("src/pages/docs/v3-upgrade.mdx", 3));
        assert!(!later_major_file("docs/300-upgrade-guides/upgrading-to-v6.mdx", 6));
        assert!(page_file("src/app/(docs)/docs/installation/(tabs)/using-vite/page.tsx") && !page_file("src/app/(docs)/docs/installation/(tabs)/layout.tsx"));
    }
}

//! Upstream docs: many packages ship no documentation (Next.js, Django,
//! FastAPI...). Their repositories do, under the exact version's git tag.
//! This module finds the repository from the package's own metadata, finds
//! the tag for the pinned version, and downloads only the docs folders
//! (Markdown, MDX, reStructuredText) from GitHub into the cache, once.
//! Network use is opt-in: `lockdocs fetch`, `--fetch` or `LOCKDOCS_FETCH=1`.

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub repo: String,
    pub tag: Option<String>,
    pub files: usize,
    pub bytes: u64,
    /// Why nothing was downloaded, when files == 0.
    pub note: Option<String>,
    /// A separate docs-site repository also included (see `DOCS_SITES`).
    #[serde(default)]
    pub site: Option<String>,
}

/// Packages whose docs live in a separate website repository. Paths with
/// `{major}` are versioned and used for any major; the others track the
/// current release and are used only when the pinned major is the latest.
struct DocsSite {
    names: &'static [&'static str],
    repo: (&'static str, &'static str),
    branch: &'static str,
    versioned: &'static [&'static str],
    latest_only: &'static [&'static str],
}

const DOCS_SITES: &[DocsSite] = &[
    DocsSite {
        names: &["react", "react-dom", "@types/react"],
        repo: ("reactjs", "react.dev"),
        branch: "main",
        versioned: &[],
        latest_only: &["src/content/reference", "src/content/learn"],
    },
    DocsSite {
        names: &["express", "@types/express"],
        repo: ("expressjs", "expressjs.com"),
        branch: "main",
        versioned: &["src/content/api/{major}x"],
        latest_only: &["src/content/docs/en"],
    },
    DocsSite {
        names: &["tailwindcss"],
        repo: ("tailwindlabs", "tailwindcss.com"),
        branch: "main",
        versioned: &[],
        latest_only: &["src/docs"],
    },
    DocsSite {
        names: &["prisma", "@prisma/client"],
        repo: ("prisma", "docs"),
        branch: "main",
        versioned: &[],
        latest_only: &["apps/docs/content/docs/orm"],
    },
];

fn npm_latest_major(agent: &ureq::Agent, name: &str) -> Option<u64> {
    let mut res = agent
        .get(&format!("https://registry.npmjs.org/{}/latest", name.replace('/', "%2F")))
        .call()
        .ok()?;
    let mut body = String::new();
    res.body_mut().as_reader().take(1 << 20).read_to_string(&mut body).ok()?;
    let v: Value = serde_json::from_str(&body).ok()?;
    v.get("version")?.as_str()?.split('.').next()?.parse().ok()
}

/// Files to take from a docs-site repository for this version, with a label.
/// Repository, branch, label and files of a docs site.
type SiteFiles = (Repo, String, String, Vec<(String, u64)>);

fn site_files(agent: &ureq::Agent, dep: &Dep) -> Result<Option<SiteFiles>> {
    if dep.eco != Eco::Npm {
        return Ok(None);
    }
    let Some(site) = DOCS_SITES.iter().find(|s| s.names.contains(&dep.name.as_str())) else {
        return Ok(None);
    };
    let major: u64 = dep.version.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    let lookup = dep.name.strip_prefix("@types/").unwrap_or(&dep.name);
    let latest = npm_latest_major(agent, lookup);
    let mut dirs: Vec<String> = site.versioned.iter().map(|d| d.replace("{major}", &major.to_string())).collect();
    let is_latest = latest == Some(major);
    if is_latest {
        dirs.extend(site.latest_only.iter().map(|d| d.to_string()));
    }
    if dirs.is_empty() {
        return Ok(None);
    }
    let repo = Repo {
        owner: site.repo.0.into(),
        name: site.repo.1.into(),
        subdir: None,
    };
    let Some(v) = api(agent, &format!("/repos/{}/{}/git/trees/{}?recursive=1", repo.owner, repo.name, site.branch))? else {
        return Ok(None);
    };
    let sha = v.get("sha").and_then(|s| s.as_str()).unwrap_or("").chars().take(7).collect::<String>();
    let mut files = Vec::new();
    for i in v.get("tree").and_then(|t| t.as_array()).into_iter().flatten() {
        let (Some(p), Some("blob")) = (i.get("path").and_then(|p| p.as_str()), i.get("type").and_then(|t| t.as_str())) else {
            continue;
        };
        let size = i.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
        if size <= MAX_FILE && doc_file(p) && !p.ends_with(".txt") && dirs.iter().any(|d| p.starts_with(&format!("{d}/"))) {
            files.push((p.to_string(), size));
        }
    }
    let label = format!(
        "github.com/{}/{}@{} ({}{})",
        repo.owner,
        repo.name,
        site.branch,
        sha,
        if is_latest {
            ", current docs; your major is the latest"
        } else {
            ", versioned API pages"
        }
    );
    Ok(Some((repo, site.branch.to_string(), label, files)))
}

fn download(agent: &ureq::Agent, repo: &Repo, reference: &str, files: &[(String, u64)], out: &Path) -> Result<Vec<u64>> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(16).build()?;
    let results: Vec<Result<u64>> = pool.install(|| {
        files
            .par_iter()
            .map(|(p, _)| {
                let url = format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/{}",
                    repo.owner,
                    repo.name,
                    enc(reference),
                    enc_path(p)
                );
                let mut res = agent.get(&url).call()?;
                if res.status().as_u16() != 200 {
                    bail!("HTTP {} for {p}", res.status());
                }
                let mut buf = Vec::new();
                res.body_mut().as_reader().take(MAX_FILE + 1).read_to_end(&mut buf)?;
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
    Ok(results.into_iter().filter_map(|r| r.ok()).collect())
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
    if owner.is_empty() || name.is_empty() {
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

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .http_status_as_error(false)
        .user_agent(concat!("lockdocs/", env!("CARGO_PKG_VERSION"), " (+https://github.com/SylphxAI/lockdocs)"))
        .build()
        .into()
}

/// GitHub API GET; Ok(None) on 404.
fn api(agent: &ureq::Agent, path: &str) -> Result<Option<Value>> {
    let mut req = agent
        .get(&format!("https://api.github.com{path}"))
        .header("Accept", "application/vnd.github+json");
    if let Some(t) = token() {
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
        bail!("GitHub API rate limit (HTTP {status}); set GITHUB_TOKEN to raise it");
    }
    if status >= 400 {
        bail!("GitHub API HTTP {status} for {path}");
    }
    Ok(Some(serde_json::from_str(&body)?))
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

struct Item {
    path: String,
    kind: String,
    sha: String,
    size: u64,
}

fn tree(agent: &ureq::Agent, repo: &Repo, sha_or_ref: &str, recursive: bool) -> Result<Option<Vec<Item>>> {
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
    let repo = repo_of(dep, src).context("no GitHub repository in the package metadata")?;
    let out = dir(dep);
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    let agent = agent();
    let label = format!("github.com/{}/{}", repo.owner, repo.name);
    let mut tag = None;
    let mut root = None;
    for t in tag_candidates(dep) {
        if let Some(items) = tree(&agent, &repo, &t, false)? {
            tag = Some(t);
            root = Some(items);
            break;
        }
    }
    let (Some(tag), Some(root)) = (tag, root) else {
        let mut m = Manifest {
            repo: label,
            tag: None,
            files: 0,
            bytes: 0,
            note: Some(format!("no git tag found for {}", dep.version)),
            site: None,
        };
        if let Ok(Some((srepo, branch, slabel, sfiles))) = site_files(&agent, dep) {
            let got = download(&agent, &srepo, &branch, &sfiles, &out.join(&srepo.name))?;
            if !got.is_empty() {
                m.files = got.len();
                m.bytes = got.iter().sum();
                m.site = Some(slabel);
                m.note = None;
            }
        }
        std::fs::write(out.join(".lockdocs-upstream.json"), serde_json::to_string(&m)?)?;
        return Ok(m);
    };
    // Find docs roots: top-level docs dirs, and docs dirs one or two levels
    // inside website/apps/packages wrappers (monorepos keep them there).
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
                    if let Some(children) = tree(&agent, &repo, &it.sha, false)? {
                        frontier.push((full, children, depth + 1));
                    }
                }
            }
        }
    }
    for (path, sha) in roots.iter().take(4) {
        let Some(items) = tree(&agent, &repo, sha, true)? else { continue };
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
        total += s;
        total <= MAX_BYTES
    });
    files.truncate(MAX_FILES);
    let ok = download(&agent, &repo, &tag, &files, &out)?;
    let failed = files.len() - ok.len();
    // A separate docs-site repository, when the package keeps its docs there.
    let mut site = None;
    let mut site_n = 0usize;
    let mut site_bytes = 0u64;
    if let Ok(Some((srepo, branch, slabel, sfiles))) = site_files(&agent, dep) {
        let got = download(&agent, &srepo, &branch, &sfiles, &out.join(&srepo.name))?;
        site_n = got.len();
        site_bytes = got.iter().sum();
        if site_n > 0 {
            site = Some(slabel);
        }
    }
    let m = Manifest {
        repo: label,
        tag: Some(tag),
        files: ok.len() + site_n,
        bytes: ok.iter().sum::<u64>() + site_bytes,
        note: if ok.is_empty() && site_n == 0 {
            Some("no docs folder at that tag".into())
        } else if failed > 0 {
            Some(format!("{failed} files failed to download"))
        } else {
            None
        },
        site,
    };
    std::fs::write(out.join(".lockdocs-upstream.json"), serde_json::to_string(&m)?)?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn github_urls() {
        let g = |s: &str| parse_github(s).map(|(o, n)| format!("{o}/{n}"));
        assert_eq!(g("git+https://github.com/vercel/next.js.git").as_deref(), Some("vercel/next.js"));
        assert_eq!(g("https://github.com/pydantic/pydantic").as_deref(), Some("pydantic/pydantic"));
        assert_eq!(g("git@github.com:colinhacks/zod.git").as_deref(), Some("colinhacks/zod"));
        assert_eq!(g("github:tokio-rs/axum").as_deref(), Some("tokio-rs/axum"));
        assert_eq!(g("expressjs/express").as_deref(), Some("expressjs/express"));
        assert_eq!(g("https://gitlab.com/x/y"), None);
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
    }
}

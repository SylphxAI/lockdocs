//! Discover the source repository from installed package metadata.

use crate::locate::Source;
use crate::{Dep, Eco};
use serde_json::Value;

use super::Repo;

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

type RepoParts = (Option<crate::private::Origin>, String, String);

/// A repository URL on github.com, or (with `any_host`) on another git host.
fn parse_repo(url: &str, any_host: bool) -> Option<RepoParts> {
    if let Some((owner, name)) = parse_github(url) {
        return Some((None, owner, name));
    }
    if !any_host {
        return None;
    }
    let (origin, path) = crate::private::git::parse_repo_url(url)?;
    let (owner, name) = path.rsplit_once('/')?;
    Some((Some(origin), owner.to_string(), name.to_string()))
}

/// The package's source repository on github.com, from its own metadata.
pub fn repo_of(dep: &Dep, src: &Source) -> Option<Repo> {
    repo_of_with(dep, src, false)
}

/// Like `repo_of`; with `any_host` a repository on another git host qualifies too.
pub fn repo_of_with(dep: &Dep, src: &Source, any_host: bool) -> Option<Repo> {
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
            let (host, owner, name) = parse_repo(&url, any_host)?;
            Some(Repo {
                owner,
                name,
                subdir: dir,
                host,
            })
        }
        Eco::PyPI => {
            let meta = std::fs::read_to_string(src.metadata.as_ref()?).ok()?;
            let mut best: Option<(u8, RepoParts)> = None;
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
                if let Some(gh) = parse_repo(&url, any_host) {
                    if best.as_ref().is_none_or(|b| rank > b.0) {
                        best = Some((rank, gh));
                    }
                }
            }
            let (_, (host, owner, name)) = best?;
            Some(Repo {
                owner,
                name,
                subdir: None,
                host,
            })
        }
        Eco::Cargo => {
            let t: toml::Table = toml::from_str(&std::fs::read_to_string(src.dir.join("Cargo.toml")).ok()?).ok()?;
            let url = t.get("package")?.get("repository")?.as_str()?;
            let (host, owner, name) = parse_repo(url, any_host)?;
            Some(Repo {
                owner,
                name,
                subdir: None,
                host,
            })
        }
        Eco::Go if any_host && !dep.name.starts_with("github.com/") && crate::private::go::is_private_module(&dep.name) => {
            let (path, sub) = crate::private::go::split_module(&dep.name)?;
            let (owner, name) = path.rsplit_once('/')?;
            let host = dep.name.split('/').next()?;
            let origin = crate::private::Url::parse(&format!("https://{host}/"))?.origin();
            Some(Repo {
                owner: owner.into(),
                name: name.into(),
                subdir: sub,
                host: Some(origin),
            })
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
                host: None,
            })
        }
    }
}

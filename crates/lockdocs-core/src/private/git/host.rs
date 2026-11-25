use super::super::{Origin, Url};
use super::{Host, Kind};

pub fn kind_of(host: &str) -> Kind {
    if host == "github.com" {
        Kind::GitHub
    } else if host.contains("gitlab") {
        Kind::GitLab
    } else if host.contains("bitbucket") {
        Kind::Bitbucket
    } else {
        Kind::GhEnterprise
    }
}

impl Host {
    pub fn of(origin: Origin) -> Host {
        let kind = kind_of(&origin.host);
        Host { origin, kind }
    }

    /// The tarball of `reference` (tag, branch or commit) and the origin that
    /// serves it.
    pub fn archive_url(&self, path: &str, reference: &str) -> String {
        let r = enc(reference);
        match self.kind {
            Kind::GitHub => format!("https://api.github.com/repos/{path}/tarball/{r}"),
            Kind::GhEnterprise => format!("{}/api/v3/repos/{path}/tarball/{r}", self.origin),
            Kind::GitLab => format!("{}/api/v4/projects/{}/repository/archive.tar.gz?sha={r}", self.origin, enc(path)),
            Kind::Bitbucket => format!("{}/{path}/get/{r}.tar.gz", self.origin),
        }
    }
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A repository URL (`git+https://h/o/r.git`, `https://h/o/r`, `git@h:o/r.git`,
/// `ssh://git@h/o/r`) as its https origin and `owner/name` path. Only URLs that
/// look like git repositories on a host other than github.com qualify.
pub fn parse_repo_url(url: &str) -> Option<(Origin, String)> {
    let u = url.trim();
    let u = u.strip_prefix("git+").unwrap_or(u);
    let (host_port, path, explicit_git) = if let Some(rest) = u.strip_prefix("git@") {
        let (h, p) = rest.split_once(':')?;
        (h.to_string(), p.to_string(), true)
    } else if let Some(rest) = u.strip_prefix("ssh://").or_else(|| u.strip_prefix("git://")) {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        let (h, p) = rest.split_once('/')?;
        (h.split(':').next()?.to_string(), p.to_string(), true)
    } else {
        let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://"))?;
        let (h, p) = rest.split_once('/')?;
        (h.to_string(), p.to_string(), false)
    };
    let host = host_port.rsplit('@').next()?.to_ascii_lowercase();
    if host.is_empty() || host == "github.com" || host == "www.github.com" {
        return None;
    }
    let path = path.split(['#', '?']).next()?.trim_matches('/');
    let had_git = path.ends_with(".git");
    let path = path.trim_end_matches(".git");
    let kind = kind_of(&host);
    let looks_like_repo = explicit_git || had_git || kind == Kind::GitLab || kind == Kind::Bitbucket || host.contains("github") || host.starts_with("git.");
    if !looks_like_repo {
        return None;
    }
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // GitLab groups nest; elsewhere owner/name. Stop at a web-UI marker.
    let cut = segs.iter().position(|s| *s == "-" || *s == "tree" || *s == "blob").unwrap_or(segs.len());
    let segs = &segs[..cut];
    if segs.len() < 2 || (kind != Kind::GitLab && segs.len() > 2) {
        return None;
    }
    let safe = |s: &&str| !s.is_empty() && *s != "." && *s != ".." && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    if !segs.iter().all(safe) {
        return None;
    }
    let scheme = if u.starts_with("http://") { "http" } else { "https" };
    let url = Url::parse(&format!("{scheme}://{host}/"))?;
    Some((url.origin(), segs.join("/")))
}

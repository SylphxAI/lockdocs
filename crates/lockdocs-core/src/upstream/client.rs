//! Configure upstream endpoints, request budgets, and credential routing.

use std::io::Read;

use anyhow::{bail, Result};
use serde_json::Value;

/// Largest compressed archive streamed from one repository by default. A
/// bigger one is not downloaded whole: the repository falls back to per-file
/// requests (see the measurements in the PR for the choice).
pub(super) const MAX_ARCHIVE: u64 = 150 << 20;

/// The cap for a package's own archive in the automatic (anonymous, 45 s
/// budget) mode: a bigger one falls back to per-file requests.
pub(super) const MAX_ARCHIVE_AUTOMATIC: u64 = 64 << 20;

fn token() -> Option<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
}

/// The GitHub hosts requests go to (replaced by a local server in tests).
#[derive(Clone)]
pub(super) struct Hosts {
    pub(super) api: String,
    pub(super) git: String,
    pub(super) codeload: String,
    pub(super) raw: String,
    pub(super) npm: String,
}

impl Hosts {
    fn github() -> Self {
        Hosts {
            api: "https://api.github.com".into(),
            git: "https://github.com".into(),
            codeload: "https://codeload.github.com".into(),
            raw: "https://raw.githubusercontent.com".into(),
            npm: "https://registry.npmjs.org".into(),
        }
    }

    /// Endpoints of another git host; archives go through `Host::archive_url`.
    fn for_host(h: &crate::private::git::Host) -> Self {
        let o = h.origin.to_string();
        Hosts {
            api: if h.kind == crate::private::git::Kind::GhEnterprise {
                format!("{o}/api/v3")
            } else {
                o.clone()
            },
            git: o.clone(),
            codeload: o.clone(),
            raw: o,
            npm: "https://registry.npmjs.org".into(),
        }
    }
}

pub(super) struct Client {
    pub(super) agent: ureq::Agent,
    pub(super) authenticated: bool,
    /// lockdocs Pro "private sources": when true, the credential for this
    /// repository's host is also sent to its git, archive and API endpoints.
    /// False for free lockdocs and for automatic first-use fetches.
    pub(super) credentialed_sources: bool,
    /// The repository's host when it is not github.com.
    pub(super) git_host: Option<crate::private::git::Host>,
    /// Credentials for that host, each bound to its own origin.
    pub(super) host_creds: crate::private::Creds,
    pub(super) deadline: Option<std::time::Instant>,
    pub(super) hosts: Hosts,
    /// Largest compressed archive streamed from one repository.
    pub(super) max_archive: u64,
}

impl std::ops::Deref for Client {
    type Target = ureq::Agent;
    fn deref(&self) -> &Self::Target {
        &self.agent
    }
}

impl Client {
    pub(super) fn check_deadline(&self) -> Result<()> {
        if self.deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            bail!("automatic upstream fetch time budget exhausted");
        }
        Ok(())
    }
}

pub(super) fn agent(automatic: bool, private: Option<(Option<crate::private::git::Host>, crate::private::Creds)>) -> Client {
    let (git_host, host_creds) = match private {
        Some((h, c)) => (h, c),
        None => (None, crate::private::Creds::default()),
    };
    let hosts = match &git_host {
        Some(h) => Hosts::for_host(h),
        None => Hosts::github(),
    };
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
        credentialed_sources: !automatic && !host_creds.0.is_empty(),
        git_host,
        host_creds,
        deadline: automatic.then(|| std::time::Instant::now() + std::time::Duration::from_secs(45)),
        hosts,
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

    /// The `Authorization` value for a git, archive or raw request, or `None`.
    /// A credential is only ever returned for the exact origin it is bound to.
    pub(super) fn source_header(&self, url: &str, smart_git: bool) -> Option<String> {
        if !(self.authenticated && self.credentialed_sources) {
            return None;
        }
        let u = crate::private::Url::parse(url)?;
        if let Some(auth) = self.host_creds.for_url(&u) {
            return Some(match (&self.git_host, smart_git) {
                (Some(h), true) => crate::private::git::git_header(h, auth),
                (None, true) => match auth {
                    crate::private::Auth::Bearer(t) => basic(t),
                    other => other.header(),
                },
                _ => auth.header(),
            });
        }
        if self.git_host.is_some() || !self.host_creds.0.is_empty() {
            return None;
        }
        // github.com with the token variable: only to the git and codeload hosts.
        let t = self.source_auth()?;
        let allowed = [&self.hosts.git, &self.hosts.codeload]
            .iter()
            .any(|h| crate::private::Url::parse(h).is_some_and(|b| b.origin() == u.origin()));
        allowed.then(|| if smart_git { basic(&t) } else { format!("Bearer {t}") })
    }
}

/// GitHub API GET; Ok(None) on 404.
pub(super) fn api(agent: &Client, path: &str) -> Result<Option<Value>> {
    agent.check_deadline()?;
    let mut url = format!("{}{path}", agent.hosts.api);
    let mut moved = false;
    let mut res = loop {
        let mut req = agent.get(&url).header("Accept", "application/vnd.github+json");
        if let Some(t) = agent.authenticated.then(token).flatten() {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let res = req.call()?;
        // A renamed repository (prisma/docs) answers the REST API with a redirect to
        // its id-based URL. The automatic client follows no redirects, so follow this
        // one, once, and only to the same API host.
        let next = res.headers().get("location").and_then(|l| l.to_str().ok()).map(String::from);
        match (res.status().as_u16(), next) {
            (301 | 302 | 307 | 308, Some(l)) if !moved && l.starts_with(&format!("{}/", agent.hosts.api)) => {
                moved = true;
                url = l;
            }
            _ => break res,
        }
    };
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

/// Basic credentials for GitHub's git endpoints (`x-access-token:<token>`).
pub(super) fn basic(token: &str) -> String {
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

pub(super) fn enc(s: &str) -> String {
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

pub(super) fn enc_path(p: &str) -> String {
    p.split('/').map(enc).collect::<Vec<_>>().join("/")
}

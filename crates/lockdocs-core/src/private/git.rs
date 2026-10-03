//! Git hosts (GitHub, GitHub Enterprise, GitLab, Bitbucket, any https git
//! host): where a repository's archive lives, and the credential for exactly
//! that host: a token variable, else what `git credential fill` returns.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    GitHub,
    GhEnterprise,
    GitLab,
    Bitbucket,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub origin: Origin,
    pub kind: Kind,
}

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

    /// The smart-HTTP endpoint that lists refs.
    pub fn refs_url(&self, path: &str) -> String {
        format!("{}/{path}.git/git-upload-pack", self.origin)
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

    /// The user name Basic auth uses with a bare token on the git endpoint.
    fn git_user(&self) -> &'static str {
        match self.kind {
            Kind::GitHub | Kind::GhEnterprise => "x-access-token",
            Kind::GitLab => "oauth2",
            Kind::Bitbucket => "x-token-auth",
        }
    }

    /// Origins the credential of this host is bound to (GitHub serves the
    /// same token from three hosts).
    fn origins(&self) -> Vec<Origin> {
        let mut v = vec![self.origin.clone()];
        if self.kind == Kind::GitHub {
            for h in ["api.github.com", "codeload.github.com"] {
                v.push(Origin {
                    scheme: "https".into(),
                    host: h.into(),
                    port: 443,
                });
            }
        }
        v
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

/// The value of the `Authorization` header for the smart-HTTP endpoint: a bare
/// token becomes Basic with the host's conventional user name.
pub fn git_header(host: &Host, auth: &Auth) -> String {
    match auth {
        Auth::Bearer(t) => Auth::Basic(host.git_user().into(), t.clone()).header(),
        other => other.header(),
    }
}

fn strip_scheme(h: &str) -> String {
    h.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

/// The token variable that belongs to this host, if the user set one.
fn env_auth(host: &Host, env: &Env) -> Option<Auth> {
    let h = &host.origin.host;
    match host.kind {
        Kind::GitHub => ["GH_TOKEN", "GITHUB_TOKEN"].iter().find_map(|k| env.get(k)).map(Auth::Bearer),
        Kind::GhEnterprise => (env.get("GH_HOST").map(|g| strip_scheme(&g)).as_deref() == Some(h.as_str()))
            .then(|| ["GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"].iter().find_map(|k| env.get(k)))
            .flatten()
            .map(Auth::Bearer),
        Kind::GitLab => (h == "gitlab.com" || env.get("GITLAB_HOST").map(|g| strip_scheme(&g)).as_deref() == Some(h.as_str()))
            .then(|| env.get("GITLAB_TOKEN"))
            .flatten()
            .map(Auth::Bearer),
        Kind::Bitbucket => env
            .get("BITBUCKET_TOKEN")
            .map(Auth::Bearer)
            .or_else(|| Some(Auth::Basic(env.get("BITBUCKET_USERNAME")?, env.get("BITBUCKET_APP_PASSWORD")?))),
    }
}

/// `git credential fill` for one origin: what the user's credential helpers
/// hold for that host. Never prompts; gives up after 15 seconds.
pub fn credential_fill(origin: &Origin, extra_env: &[(&str, &str)]) -> Option<(String, String)> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let default = if origin.scheme == "https" { 443 } else { 80 };
    let host = if origin.port == default {
        origin.host.clone()
    } else {
        format!("{}:{}", origin.host, origin.port)
    };
    let mut cmd = Command::new("git");
    cmd.args(["credential", "fill"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().ok()?;
    child
        .stdin
        .take()?
        .write_all(format!("protocol={}\nhost={host}\n\n", origin.scheme).as_bytes())
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut s);
        s
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    let out = reader.join().ok()?;
    let get = |k: &str| out.lines().find_map(|l| l.strip_prefix(&format!("{k}=")).map(String::from));
    Some((get("username")?, get("password")?))
}

/// The credential for `host`, bound to its origin(s): a token variable, then
/// (when `fill`) the user's git credential helpers.
pub fn credentials(host: &Host, env: &Env, fill: bool) -> Creds {
    let auth = env_auth(host, env).or_else(|| fill.then(|| credential_fill(&host.origin, &[])).flatten().map(|(u, p)| Auth::Basic(u, p)));
    let mut creds = Creds::default();
    if let Some(a) = auth {
        for o in host.origins() {
            creds.push(Credential::new(o, a.clone()));
        }
    }
    creds
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

#[cfg(test)]
mod tests {
    use super::*;

    fn host(u: &str) -> Host {
        Host::of(Url::parse(u).unwrap().origin())
    }

    #[test]
    fn repo_urls_on_other_hosts() {
        let p = |u: &str| parse_repo_url(u).map(|(o, p)| (o.to_string(), p));
        assert_eq!(
            p("git+https://git.acme.example/team/lib.git"),
            Some(("https://git.acme.example".into(), "team/lib".into()))
        );
        assert_eq!(
            p("https://gitlab.acme.example/grp/sub/lib"),
            Some(("https://gitlab.acme.example".into(), "grp/sub/lib".into()))
        );
        assert_eq!(p("git@bitbucket.org:ws/repo.git"), Some(("https://bitbucket.org".into(), "ws/repo".into())));
        assert_eq!(p("https://github.com/o/r"), None);
        // A docs site is not a repository.
        assert_eq!(p("https://foo.readthedocs.io/en/latest"), None);
        assert_eq!(p("https://example.com/o/r"), None);
    }

    #[test]
    fn env_tokens_belong_to_their_own_host() {
        let e = Env::fixed(
            &[
                ("GH_TOKEN", "SECRET-GH"),
                ("GITLAB_TOKEN", "SECRET-GL"),
                ("GITLAB_HOST", "https://gitlab.acme.example"),
            ],
            None,
        );
        let gh = credentials(&host("https://github.com/"), &e, false);
        assert!(gh.for_url(&Url::parse("https://codeload.github.com/o/r/tar.gz/x").unwrap()).is_some());
        assert!(gh.for_url(&Url::parse("https://git.acme.example/o/r").unwrap()).is_none());
        // GH_TOKEN is never offered to another GitHub-style host.
        assert!(credentials(&host("https://github.acme.example/"), &e, false).0.is_empty());
        let gl = credentials(&host("https://gitlab.acme.example/"), &e, false);
        assert_eq!(
            gl.for_url(&Url::parse("https://gitlab.acme.example/api/v4/x").unwrap()).unwrap().header(),
            "Bearer SECRET-GL"
        );
        assert!(credentials(&host("https://gitlab.other.example/"), &e, false).0.is_empty());
        assert!(!format!("{gh:?}{gl:?}").contains("SECRET"));
    }

    #[test]
    fn git_endpoint_uses_basic_with_the_host_user() {
        let h = host("https://gitlab.acme.example/");
        assert_eq!(git_header(&h, &Auth::Bearer("t".into())), Auth::Basic("oauth2".into(), "t".into()).header());
        assert_eq!(
            git_header(&h, &Auth::Basic("u".into(), "p".into())),
            Auth::Basic("u".into(), "p".into()).header()
        );
    }

    #[test]
    fn archive_urls() {
        assert_eq!(
            host("https://github.com/").archive_url("o/r", "v1.0.0"),
            "https://api.github.com/repos/o/r/tarball/v1.0.0"
        );
        assert_eq!(
            host("https://gitlab.x.example/").archive_url("g/s/r", "v1"),
            "https://gitlab.x.example/api/v4/projects/g%2Fs%2Fr/repository/archive.tar.gz?sha=v1"
        );
        assert_eq!(
            host("https://bitbucket.org/").archive_url("w/r", "v1"),
            "https://bitbucket.org/w/r/get/v1.tar.gz"
        );
        assert_eq!(
            host("https://git.x.example/").archive_url("o/r", "v1"),
            "https://git.x.example/api/v3/repos/o/r/tarball/v1"
        );
    }

    #[test]
    fn credential_fill_reads_the_helper_for_that_host() {
        if std::process::Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let o = Url::parse("https://git.acme.example/").unwrap().origin();
        let got = credential_fill(
            &o,
            &[
                ("GIT_CONFIG_GLOBAL", "/dev/null"),
                ("GIT_CONFIG_SYSTEM", "/dev/null"),
                ("GIT_CONFIG_COUNT", "1"),
                ("GIT_CONFIG_KEY_0", "credential.helper"),
                ("GIT_CONFIG_VALUE_0", "!f() { echo username=me; echo password=SECRET-FILL; }; f"),
            ],
        );
        assert_eq!(got, Some(("me".into(), "SECRET-FILL".into())));
    }
}

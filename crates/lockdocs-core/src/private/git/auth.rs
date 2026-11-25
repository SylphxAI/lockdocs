use super::super::{Auth, Credential, Creds, Env, Origin};
use super::{Host, Kind};

impl Host {
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

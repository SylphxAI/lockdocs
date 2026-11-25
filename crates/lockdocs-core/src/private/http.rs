//! HTTP reads that select credentials again on each redirect hop.

use super::{strip_userinfo, Creds, Url};
use anyhow::{bail, Result};
use std::io::Read;

const MAX_BODY: u64 = 64 * 1024 * 1024;
const MAX_HOPS: usize = 5;

pub struct Http {
    agent: ureq::Agent,
    pub creds: Creds,
}

pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    /// The URL that finally answered (credentials stripped).
    pub url: String,
}

impl Http {
    pub fn new(creds: Creds) -> Http {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(concat!("lockdocs/", env!("CARGO_PKG_VERSION"), " (+https://github.com/SylphxAI/lockdocs)"))
            .build()
            .into();
        Http { agent, creds }
    }

    /// GET with credentials bound to their origin. Redirects are followed here,
    /// one hop at a time, and the credential is looked up again for each hop.
    pub fn get(&self, url: &str, accept: Option<&str>) -> Result<Reply> {
        let Some(mut cur) = Url::parse(url) else {
            bail!("not an http(s) URL: {}", strip_userinfo(url));
        };
        for _ in 0..=MAX_HOPS {
            let mut req = self.agent.get(&cur.clean());
            if let Some(a) = accept {
                req = req.header("Accept", a);
            }
            if let Some(auth) = self.creds.for_url(&cur) {
                req = req.header("Authorization", &auth.header());
            }
            let mut res = match req.call() {
                Ok(r) => r,
                Err(e) => bail!("GET {}: {}", cur.clean(), self.creds.redact(&e.to_string())),
            };
            let status = res.status().as_u16();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let loc = res.headers().get("location").and_then(|v| v.to_str().ok()).map(String::from);
                let Some(next) = loc.and_then(|l| cur.join(&l)) else {
                    bail!("GET {}: redirect without a usable Location", cur.clean());
                };
                cur = next;
                continue;
            }
            let mut body = Vec::new();
            res.body_mut().as_reader().take(MAX_BODY + 1).read_to_end(&mut body)?;
            if body.len() as u64 > MAX_BODY {
                bail!("{} is larger than {} MB", cur.clean(), MAX_BODY / 1024 / 1024);
            }
            return Ok(Reply {
                status,
                body,
                url: cur.clean(),
            });
        }
        bail!("GET {}: too many redirects", strip_userinfo(url))
    }

    /// A 200 body, `None` on 404; 401/403 and other statuses are errors that
    /// say which origin refused, never what was sent.
    pub fn get_ok(&self, url: &str, accept: Option<&str>) -> Result<Option<Vec<u8>>> {
        let r = self.get(url, accept)?;
        match r.status {
            200 => Ok(Some(r.body)),
            404 | 410 => Ok(None),
            401 | 403 => {
                let host = Url::parse(&r.url).map_or_else(|| "the registry".to_string(), |u| u.origin().to_string());
                let sent = Url::parse(&r.url).is_some_and(|u| self.creds.for_url(&u).is_some());
                bail!(
                    "{host} refused the request (HTTP {}){}",
                    r.status,
                    if sent {
                        "; the configured credential for that host was rejected, check that it is current"
                    } else {
                        "; no credential is configured for that host"
                    }
                )
            }
            s => bail!("{}: HTTP {s}", r.url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{testutil::serve, Auth, Credential};
    use super::{Creds, Http, Url};

    #[test]
    fn redirect_to_another_host_drops_the_credential() {
        // The "evil" server records what it receives; the "good" one redirects to it.
        let evil = serve(|_, _| (200, "text/plain".into(), b"ok".to_vec(), vec![]));
        let target = format!("http://localhost:{}/steal", evil.host.rsplit(':').next().unwrap());
        let good = serve(move |p, _| {
            if p == "/start" {
                (302, "text/plain".into(), vec![], vec![("Location".into(), target.clone())])
            } else {
                (404, "text/plain".into(), vec![], vec![])
            }
        });
        let mut creds = Creds::default();
        creds.push(Credential::new(Url::parse(&good.base).unwrap().origin(), Auth::Bearer("SECRET-TOKEN-2".into())));
        let http = Http::new(creds);
        let r = http.get(&format!("{}/start", good.base), None).unwrap();
        assert_eq!(r.status, 200);
        let good_seen = good.seen.lock().unwrap().clone();
        assert_eq!(good_seen[0].1.as_deref(), Some("Bearer SECRET-TOKEN-2"));
        let evil_seen = evil.seen.lock().unwrap().clone();
        assert_eq!(evil_seen.len(), 1);
        assert_eq!(evil_seen[0].1, None, "credential followed a redirect to another host");
    }

    #[test]
    fn rejected_credential_error_names_host_not_secret() {
        let m = serve(|_, _| (401, "text/plain".into(), vec![], vec![]));
        let mut creds = Creds::default();
        creds.push(Credential::new(Url::parse(&m.base).unwrap().origin(), Auth::Bearer("SECRET-TOKEN-3".into())));
        let e = Http::new(creds).get_ok(&format!("{}/x", m.base), None).unwrap_err().to_string();
        assert!(e.contains("rejected") && !e.contains("SECRET"), "{e}");
    }
}

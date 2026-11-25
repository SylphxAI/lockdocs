//! Origin-bound credentials, secret redaction, and netrc lookup.

use super::{b64_encode, read, Env, Origin, Url};
use std::path::PathBuf;

/// How a secret is presented. Prints as `***`.
#[derive(Clone, PartialEq, Eq)]
pub enum Auth {
    Bearer(String),
    Basic(String, String),
    /// The `Authorization` value as-is (Cargo registry tokens).
    Raw(String),
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Auth(***)")
    }
}

impl Auth {
    /// The `Authorization` header value.
    pub fn header(&self) -> String {
        match self {
            Auth::Bearer(t) => format!("Bearer {t}"),
            Auth::Basic(u, p) => format!("Basic {}", b64_encode(format!("{u}:{p}").as_bytes())),
            Auth::Raw(v) => v.clone(),
        }
    }
    /// Every substring that must never be printed.
    fn secrets(&self) -> Vec<String> {
        match self {
            Auth::Bearer(t) | Auth::Raw(t) => vec![t.clone()],
            Auth::Basic(u, p) => vec![p.clone(), b64_encode(format!("{u}:{p}").as_bytes())],
        }
    }
}

/// A secret bound to one origin (and optionally a path prefix on it).
#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    pub origin: Origin,
    pub path_prefix: Option<String>,
    pub auth: Auth,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Credential({}, ***)", self.origin)
    }
}

impl Credential {
    pub fn new(origin: Origin, auth: Auth) -> Credential {
        Credential {
            origin,
            path_prefix: None,
            auth,
        }
    }
    fn matches(&self, url: &Url) -> bool {
        self.origin == url.origin() && self.path_prefix.as_ref().is_none_or(|p| url.path.starts_with(p.as_str()))
    }
}

/// The credentials in play for one operation.
#[derive(Clone, Default, Debug)]
pub struct Creds(pub Vec<Credential>);

impl Creds {
    pub fn push(&mut self, c: Credential) {
        if !self.0.contains(&c) {
            self.0.push(c);
        }
    }
    /// The authorization for exactly this URL's origin, and nothing else.
    /// The most specific path prefix wins.
    pub fn for_url(&self, url: &Url) -> Option<&Auth> {
        self.0
            .iter()
            .filter(|c| c.matches(url))
            .max_by_key(|c| c.path_prefix.as_ref().map_or(0, String::len))
            .map(|c| &c.auth)
    }
    /// `text` with every secret and every URL userinfo replaced by `***`.
    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_string();
        for c in &self.0 {
            for s in c.auth.secrets() {
                if s.len() >= 3 {
                    out = out.replace(&s, "***");
                }
            }
        }
        strip_userinfo(&out)
    }
}

/// `https://user:pw@host/x` becomes `https://***@host/x`.
pub fn strip_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '/' || c == '"' || c == '\'')
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) => {
                out.push_str("***");
                out.push_str(&tail[at..]);
                rest = "";
            }
            None => {
                out.push_str(&tail[..end]);
                rest = &tail[end..];
            }
        }
        if rest.is_empty() {
            break;
        }
    }
    out.push_str(rest);
    out
}

// ---- netrc -----------------------------------------------------------------

/// `machine HOST login L password P` entries (and `default`) of a netrc file.
pub fn parse_netrc(text: &str) -> Vec<(Option<String>, String, String)> {
    let mut out = Vec::new();
    let mut toks = text.split_whitespace();
    let (mut machine, mut login, mut password): (Option<String>, Option<String>, Option<String>) = (None, None, None);
    let mut open = false;
    let flush = |m: &mut Option<String>, l: &mut Option<String>, p: &mut Option<String>, open: &mut bool, out: &mut Vec<_>| {
        if *open {
            if let (Some(l), Some(p)) = (l.take(), p.take()) {
                out.push((m.take(), l, p));
            }
        }
        *m = None;
        *l = None;
        *p = None;
        *open = false;
    };
    while let Some(t) = toks.next() {
        match t {
            "machine" => {
                flush(&mut machine, &mut login, &mut password, &mut open, &mut out);
                machine = toks.next().map(|h| h.to_ascii_lowercase());
                open = true;
            }
            "default" => {
                flush(&mut machine, &mut login, &mut password, &mut open, &mut out);
                open = true;
            }
            "login" => login = toks.next().map(String::from),
            "password" => password = toks.next().map(String::from),
            "account" => {
                toks.next();
            }
            "macdef" => {
                // A macro runs to the next blank line; this tokenizer cannot see
                // lines, so stop reading rather than misparse it.
                break;
            }
            _ => {}
        }
    }
    flush(&mut machine, &mut login, &mut password, &mut open, &mut out);
    out
}

/// The netrc credential for `url`'s host, bound to that URL's origin.
pub fn netrc_for(env: &Env, url: &Url) -> Option<Credential> {
    let path = env
        .get("NETRC")
        .map(PathBuf::from)
        .or_else(|| env.home_file(".netrc"))
        .or_else(|| env.home_file("_netrc"))?;
    let entries = parse_netrc(&read(&path)?);
    let hit = entries
        .iter()
        .find(|(m, _, _)| m.as_deref() == Some(url.host.trim_matches(['[', ']'])))
        .or_else(|| entries.iter().find(|(m, _, _)| m.is_none()))?;
    Some(Credential::new(url.origin(), Auth::Basic(hit.1.clone(), hit.2.clone())))
}

#[cfg(test)]
mod tests {
    use super::{parse_netrc, Auth, Credential, Creds, Url};

    #[test]
    fn credentials_match_only_their_origin_and_never_print() {
        let u = Url::parse("https://npm.corp.example/").unwrap();
        let mut creds = Creds::default();
        creds.push(Credential::new(u.origin(), Auth::Bearer("SECRET-TOKEN-1".into())));
        assert!(creds.for_url(&u).is_some());
        for other in [
            "https://npm.corp.example:8443/",
            "http://npm.corp.example/",
            "https://evil.example/",
            "https://npm.corp.example.evil.example/",
        ] {
            assert!(creds.for_url(&Url::parse(other).unwrap()).is_none(), "{other}");
        }
        assert!(!format!("{creds:?}").contains("SECRET"));
        assert_eq!(
            creds.redact("failed with SECRET-TOKEN-1 at https://u:pw@h.example/x"),
            "failed with *** at https://***@h.example/x"
        );
    }

    #[test]
    fn netrc_entries() {
        let e = parse_netrc("machine a.example login u1 password p1\nmachine b.example\n  login u2\n  password p2\ndefault login d password dp\n");
        assert_eq!(e.len(), 3);
        assert_eq!(e[1], (Some("b.example".into()), "u2".into(), "p2".into()));
        assert_eq!(e[2].0, None);
    }
}

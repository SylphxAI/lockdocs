//! URL parsing, origin identity, and encoding helpers for private sources.

/// A parsed http(s) URL. Only what credential binding needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub userinfo: Option<(String, Option<String>)>,
    /// Path and query, always starting with `/`.
    pub path: String,
}

impl Url {
    pub fn parse(s: &str) -> Option<Url> {
        let s = s.trim();
        let (scheme, rest) = s.split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return None;
        }
        let (auth, path) = match rest.find(['/', '?', '#']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let path = path.split('#').next().unwrap_or("");
        let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
        let (userinfo, hostport) = match auth.rfind('@') {
            Some(i) => {
                let ui = &auth[..i];
                let (u, p) = match ui.split_once(':') {
                    Some((u, p)) => (u, Some(p)),
                    None => (ui, None),
                };
                (Some((pct_decode(u), p.map(pct_decode))), &auth[i + 1..])
            }
            None => (None, auth),
        };
        let (host, port) = if let Some(r) = hostport.strip_prefix('[') {
            let (h, after) = r.split_once(']')?;
            (format!("[{}]", h.to_ascii_lowercase()), after.strip_prefix(':').and_then(|p| p.parse().ok()))
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (h.to_ascii_lowercase(), p.parse().ok()),
                _ => (hostport.to_ascii_lowercase(), None),
            }
        };
        if host.is_empty() {
            return None;
        }
        let port = port.unwrap_or(if scheme == "https" { 443 } else { 80 });
        Some(Url {
            scheme,
            host,
            port,
            userinfo,
            path,
        })
    }

    pub fn origin(&self) -> Origin {
        Origin {
            scheme: self.scheme.clone(),
            host: self.host.clone(),
            port: self.port,
        }
    }

    /// `scheme://host[:port]` plus the path, without credentials.
    pub fn clean(&self) -> String {
        format!("{}{}", self.origin(), self.path)
    }

    /// Resolve a `Location` header against this URL.
    pub fn join(&self, loc: &str) -> Option<Url> {
        if loc.contains("://") {
            return Url::parse(loc);
        }
        if let Some(rest) = loc.strip_prefix("//") {
            return Url::parse(&format!("{}://{rest}", self.scheme));
        }
        let path = if loc.starts_with('/') {
            loc.to_string()
        } else {
            let base = self.path.split('?').next().unwrap_or("/");
            let dir = &base[..base.rfind('/').map_or(0, |i| i + 1)];
            format!("{dir}{loc}")
        };
        Some(Url {
            path: normalize_path(&path),
            userinfo: None,
            ..self.clone()
        })
    }
}

/// Resolve `.` and `..` segments of a path (query kept as-is).
fn normalize_path(p: &str) -> String {
    let (path, query) = match p.split_once('?') {
        Some((a, b)) => (a, Some(b)),
        None => (p, None),
    };
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/').skip(1) {
        match seg {
            "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    let mut r = format!("/{}", out.join("/"));
    if path.ends_with("/.") || path.ends_with("/..") {
        r.push('/');
    }
    match query {
        Some(q) => format!("{r}?{q}"),
        None => r,
    }
}

/// Scheme, host and port: the unit a credential is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: u16,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let default = if self.scheme == "https" { 443 } else { 80 };
        if self.port == default {
            write!(f, "{}://{}", self.scheme, self.host)
        } else {
            write!(f, "{}://{}:{}", self.scheme, self.host, self.port)
        }
    }
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---- base64 ----------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn b64_encode(input: &[u8]) -> String {
    let mut out = String::new();
    for c in input.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub(crate) fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.trim().trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{b64_decode, b64_encode, Url};

    #[test]
    fn url_origin_and_userinfo() {
        let u = Url::parse("https://User:p%40ss@Reg.Example.com:8443/a/b?x=1#f").unwrap();
        assert_eq!(u.origin().to_string(), "https://reg.example.com:8443");
        assert_eq!(u.userinfo, Some(("User".into(), Some("p@ss".into()))));
        assert_eq!(u.clean(), "https://reg.example.com:8443/a/b?x=1");
        assert_eq!(Url::parse("https://x.com").unwrap().origin().to_string(), "https://x.com");
        assert_eq!(Url::parse("http://x.com:80/").unwrap().origin(), Url::parse("http://x.com/").unwrap().origin());
        assert_ne!(Url::parse("http://x.com/").unwrap().origin(), Url::parse("https://x.com/").unwrap().origin());
        assert_ne!(
            Url::parse("https://x.com:444/").unwrap().origin(),
            Url::parse("https://x.com/").unwrap().origin()
        );
        assert!(Url::parse("ftp://x.com/").is_none());
    }

    #[test]
    fn base64_round_trip() {
        for s in ["a", "ab", "abc", "user:pa55word", "x-access-token:ghp_abc"] {
            assert_eq!(b64_decode(&b64_encode(s.as_bytes())).unwrap(), s.as_bytes());
        }
    }
}

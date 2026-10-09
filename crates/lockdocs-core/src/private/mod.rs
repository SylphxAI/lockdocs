//! lockdocs Pro "private sources": read packages and docs from the private
//! registries and git hosts the user has already configured, with the user's
//! own credentials, each sent only to the host it belongs to.
//!
//! Nothing here decides whether the caller may use Pro: the binary checks the
//! licence and sets `Options::private`. This module only knows two things: how
//! to find the user's existing configuration (`.npmrc`, uv/pip config,
//! `.cargo/config.toml`, `GOPROXY`, `~/.netrc`, token variables,
//! `git credential fill`), and how to keep a credential on its own host:
//!
//! * a `Credential` carries the exact origin (scheme, host, port) it was
//!   configured for, and `Creds::for_url` returns it only for that origin;
//! * `Http` follows redirects itself, one hop at a time, and asks `Creds` again
//!   for every hop, so a redirect to another host loses the header;
//! * `Auth` and `Credential` print as `***`, and every error that reaches the
//!   user passes through `Creds::redact`.

pub mod cargo;
pub mod git;
pub mod go;
pub mod npm;
pub mod pypi;

use crate::{Dep, Eco};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The feature name a Pro-less call reports in the `pro_required` answer.
pub const FEATURE: &str = "Private sources";

/// A request needed a private source and the licence does not cover it.
#[derive(Debug, Clone)]
pub struct PrivateRequired {
    pub package: String,
    /// The host (never a credential) the package lives on.
    pub host: String,
}

impl std::fmt::Display for PrivateRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is read from {}, a source outside the public defaults; reading private sources is part of lockdocs Pro",
            self.package, self.host
        )
    }
}

impl std::error::Error for PrivateRequired {}

/// The process environment, or a fixed one for tests.
#[derive(Clone)]
pub struct Env {
    vars: Option<HashMap<String, String>>,
    pub home: Option<PathBuf>,
}

impl Env {
    pub fn process() -> Env {
        Env {
            vars: None,
            home: dirs::home_dir(),
        }
    }
    pub fn fixed(vars: &[(&str, &str)], home: Option<&Path>) -> Env {
        Env {
            vars: Some(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()),
            home: home.map(Path::to_path_buf),
        }
    }
    /// A variable, `None` when unset or empty.
    pub fn get(&self, k: &str) -> Option<String> {
        let v = match &self.vars {
            Some(m) => m.get(k).cloned(),
            None => std::env::var(k).ok(),
        };
        v.filter(|v| !v.is_empty())
    }
    /// Every variable whose name starts with `prefix` (case-insensitive).
    pub fn with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let p = prefix.to_ascii_uppercase();
        let all: Vec<(String, String)> = match &self.vars {
            Some(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            None => std::env::vars().collect(),
        };
        let mut out: Vec<_> = all
            .into_iter()
            .filter(|(k, _)| k.to_ascii_uppercase().starts_with(&p) && !k.is_empty())
            .collect();
        out.sort();
        out
    }
    pub fn home_file(&self, rel: &str) -> Option<PathBuf> {
        self.home.as_ref().map(|h| h.join(rel))
    }
}

pub(crate) fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok()
}

// ---- URLs and origins ------------------------------------------------------

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

// ---- credentials -----------------------------------------------------------

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

// ---- HTTP with host-bound credentials ---------------------------------------

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

// ---- plans ------------------------------------------------------------------

/// One place a package may be read from.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Registry, index or proxy base URL (no credentials in it).
    pub base: String,
    /// Not one of the public defaults.
    pub private: bool,
    /// The user's config or the lockfile says this package belongs here, so a
    /// public lookup would be wrong (or would leak the name).
    pub definitive: bool,
    /// The exact artifact URL the lockfile recorded, when it did.
    pub pinned: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub candidates: Vec<Candidate>,
    pub creds: Creds,
}

impl Plan {
    pub fn private_host(&self) -> Option<String> {
        self.candidates
            .iter()
            .find(|c| c.private)
            .and_then(|c| Url::parse(&c.base).map(|u| u.origin().host))
    }
    /// Needs a private source for sure: the first choice is private and definitive.
    pub fn definitely_private(&self) -> bool {
        self.candidates.first().is_some_and(|c| c.private && c.definitive)
    }
    pub fn any_private(&self) -> bool {
        self.candidates.iter().any(|c| c.private)
    }
}

/// Where `dep` would be read from, per the user's own configuration and lockfiles.
pub fn plan(dep: &Dep, root: &Path, env: &Env) -> Plan {
    match dep.eco {
        Eco::Npm => npm::plan(dep, root, env),
        Eco::PyPI => pypi::plan(dep, root, env),
        Eco::Cargo => cargo::plan(dep, root, env),
        Eco::Go => go::plan(dep, root, env),
    }
}

/// Does reading `dep` need a private source for sure (no network)?
pub fn needs_private(dep: &Dep, root: &Path) -> Option<PrivateRequired> {
    let p = plan(dep, root, &Env::process());
    p.definitely_private().then(|| PrivateRequired {
        package: dep.id(),
        host: p.private_host().unwrap_or_default(),
    })
}

/// How a private download ended.
pub enum Outcome {
    /// Unpacked into the target directory; `host` is where it came from.
    Done { host: String },
    /// No private candidate applied; the caller runs the public path. Carries
    /// the host of a private candidate that was skipped without Pro, so a
    /// public failure can be reported as "needs Pro".
    UsePublic { skipped: Option<PrivateRequired> },
}

/// Run the plan: private candidates first (when `allowed`), then hand over to
/// the public path. Without `allowed`, a definitively private package fails at
/// once with `PrivateRequired`, without any network.
pub fn fetch_into(dep: &Dep, root: &Path, allowed: bool, dir: &Path) -> Result<Outcome> {
    let env = Env::process();
    let plan = plan(dep, root, &env);
    if !plan.any_private() {
        return Ok(Outcome::UsePublic { skipped: None });
    }
    let required = PrivateRequired {
        package: dep.id(),
        host: plan.private_host().unwrap_or_default(),
    };
    if !allowed {
        if plan.definitely_private() {
            return Err(required.into());
        }
        return Ok(Outcome::UsePublic { skipped: Some(required) });
    }
    let http = Http::new(plan.creds.clone());
    let mut errors = Vec::new();
    for c in &plan.candidates {
        if !c.private {
            return Ok(Outcome::UsePublic { skipped: None });
        }
        let r = match dep.eco {
            Eco::Npm => npm::download(&http, dep, c, dir),
            Eco::PyPI => pypi::download(&http, dep, c, dir),
            Eco::Cargo => cargo::download(&http, dep, c, dir),
            Eco::Go => go::download(&http, &env, dep, c, dir),
        };
        match r {
            Ok(true) => {
                return Ok(Outcome::Done {
                    host: Url::parse(&c.base).map_or_else(|| "a private source".into(), |u| u.origin().to_string()),
                })
            }
            Ok(false) => errors.push(format!(
                "{}: not found",
                Url::parse(&c.base).map_or_else(|| "private source".into(), |u| u.origin().to_string())
            )),
            Err(e) => errors.push(plan.creds.redact(&format!("{e:#}"))),
        }
    }
    bail!("{}: not available from the configured private sources ({})", dep.id(), errors.join("; "))
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    /// A request the mock saw: path and the Authorization header, if any.
    pub type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

    pub struct Mock {
        pub base: String,
        pub host: String,
        pub seen: Seen,
    }

    /// An in-process HTTP server. `route(path, authorization)` returns
    /// (status, content type, body, extra headers).
    pub fn serve<F>(route: F) -> Mock
    where
        F: Fn(&str, Option<&str>) -> (u16, String, Vec<u8>, Vec<(String, String)>) + Send + 'static,
    {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Seen = Default::default();
        let log = seen.clone();
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
                let path = head.lines().next().unwrap_or("").split(' ').nth(1).unwrap_or("").to_string();
                let auth = head.lines().find_map(|l| {
                    l.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                        .map(|(_, v)| v.trim().to_string())
                });
                log.lock().unwrap().push((path.clone(), auth.clone()));
                let (status, ctype, body, extra) = route(&path, auth.as_deref());
                let mut resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (k, v) in extra {
                    resp.push_str(&format!("{k}: {v}\r\n"));
                }
                resp.push_str("\r\n");
                let _ = c.write_all(resp.as_bytes());
                let _ = c.write_all(&body);
                let _ = c.flush();
                let _ = c.shutdown(std::net::Shutdown::Write);
            }
        });
        Mock {
            base: format!("http://{addr}"),
            host: addr.to_string(),
            seen,
        }
    }

    pub fn tgz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()));
        for (n, data) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, n, *data).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    pub fn write(p: &std::path::Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

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

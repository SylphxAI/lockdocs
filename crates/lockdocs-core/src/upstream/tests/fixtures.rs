use std::io::Read;

use anyhow::Result;

use crate::{Dep, Eco};

use super::super::client::{Client, Hosts, MAX_ARCHIVE};
use super::super::fetch::build;
use super::super::{Manifest, Repo, FORMAT};

pub(super) fn manifest() -> Manifest {
    Manifest {
        format: FORMAT,
        repo: "github.com/o/r".into(),
        tag: Some("v1.2.3".into()),
        commit: Some("0123456789abcdef0123456789abcdef01234567".into()),
        docs_sites_checked: false,
        files: 1,
        bytes: 10,
        note: None,
        site: None,
        pages: Vec::new(),
        via: None,
    }
}

pub(super) fn dep(name: &str, version: &str) -> Dep {
    Dep {
        eco: Eco::Npm,
        name: name.into(),
        version: version.into(),
        direct: true,
        from: "x".into(),
    }
}

pub(super) fn repo(subdir: Option<&str>) -> Repo {
    Repo {
        owner: "o".into(),
        name: "r".into(),
        subdir: subdir.map(String::from),
        host: None,
    }
}

/// A small `<repo>-<ref>/...` codeload-shaped archive built in memory.
pub(super) fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (p, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, format!("r-abc/{p}"), *data).unwrap();
    }
    // A symlink and a directory must never be extracted.
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(tar::EntryType::Symlink);
    h.set_size(0);
    b.append_link(&mut h, "r-abc/docs/link.md", "/etc/passwd").unwrap();
    let tar = b.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gz, &tar).unwrap();
    gz.finish().unwrap()
}

/// A local stand-in for GitHub: git smart-HTTP, codeload, REST and raw.
/// `api_blocked` answers every REST request 403, like an exhausted quota.
pub(super) struct Fake {
    base: String,
    pub(super) seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Codeload {
    Ok,
    Fail,
    /// 404 when a token is sent, the archive when anonymous.
    MissingWithAuth,
    /// The git endpoint answers 401 when a token is sent.
    BadToken,
    /// A private repository: anonymous git answers 401 and codeload 404.
    Private,
    /// Like `Private`, but the package's docs site repository answers.
    PrivateWithSite,
    /// Answers after a pause longer than a short deadline.
    Slow,
    /// Legacy docs fallback writes one file and fails another.
    PartialLegacy,
    /// Express docs site with both majors on main and no version branches.
    ExpressSite,
    ExpressSiteRest,
}

// The two English guide paths and API paths match expressjs.com at
// 264c3b928d096ea6ac49f02aefd3c4e0baae83ee; only their contents are synthetic.
pub(super) const EXPRESS_SITE_DOCS: &[(&str, &[u8])] = &[
    ("src/content/api/4x/api.mdx", b"# Express 4 API"),
    ("src/content/docs/en/4x/guide/routing.mdx", b"# Express 4 Routing"),
    ("src/content/api/5x/api.mdx", b"# Express 5 API"),
    ("src/content/docs/en/5x/guide/routing.mdx", b"# Express 5 Routing"),
    ("src/content/docs/en/40x/guide/routing.mdx", b"# Not Express 4"),
];

pub(super) const SHA: &str = "1111111111111111111111111111111111111111";

pub(super) fn fake(codeload: Codeload, api_blocked: bool, tag: Option<&str>) -> Fake {
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
    let log = seen.clone();
    let tag = tag.map(String::from);
    let site_docs_present = codeload == Codeload::PrivateWithSite;
    let site_archive = tarball(&[("src/content/learn/a.md", b"# learn")]);
    let express = matches!(codeload, Codeload::ExpressSite | Codeload::ExpressSiteRest);
    let express_oversize = "src/content/docs/en/4x/guide/oversize.mdx";
    let express_archive = if express {
        let oversized = vec![0; super::MAX_FILE as usize + 1];
        let mut docs = EXPRESS_SITE_DOCS.to_vec();
        docs.push((express_oversize, &oversized));
        tarball(&docs)
    } else {
        Vec::new()
    };
    let legacy_archive = tarball(&[
        ("content/docs/hooks-reference.md", b"# Hooks API Reference"),
        ("content/blog/new-api.md", b"# Future API"),
    ]);
    let archive = tarball(&[("README.md", b"# readme"), ("docs/guide.md", b"# guide"), ("src/lib.rs", b"fn x() {}")]);
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
            let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n").map_or(buf.len(), |i| i + 4);
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            // Always consume the request body before answering: closing a socket with unread
            // data makes macOS and Windows reset the connection under the client.
            let lower = head.to_ascii_lowercase();
            let mut body_in = buf[head_end..].to_vec();
            if lower.contains("transfer-encoding: chunked") {
                while !body_in.ends_with(b"0\r\n\r\n") {
                    let n = c.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    body_in.extend_from_slice(&chunk[..n]);
                }
            } else if let Some(len) = lower
                .lines()
                .find_map(|l| l.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
            {
                while body_in.len() < len {
                    let n = c.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    body_in.extend_from_slice(&chunk[..n]);
                }
            }
            let line = head.lines().next().unwrap_or("").to_string();
            let path = line.split(' ').nth(1).unwrap_or("").to_string();
            let authed = head.to_ascii_lowercase().contains("\r\nauthorization:");
            log.lock().unwrap().push(format!(
                "{} {}{}",
                line.split(' ').next().unwrap_or(""),
                path,
                if authed { " AUTH" } else { "" }
            ));
            let json = |v: serde_json::Value| (200, "application/json", v.to_string().into_bytes());
            let (status, ctype, body): (u16, &str, Vec<u8>) = if path.ends_with("/git-upload-pack")
                && (matches!(codeload, Codeload::Private | Codeload::PrivateWithSite) || (authed && codeload == Codeload::BadToken))
            {
                (401, "text/plain", Vec::new())
            } else if path.ends_with("/git-upload-pack") {
                let mut resp = String::new();
                if let Some(t) = tag.as_ref().filter(|_| !path.starts_with("/expressjs/expressjs.com/")) {
                    let l = format!("{SHA} refs/tags/{t}\n");
                    resp.push_str(&format!("{:04x}{l}", l.len() + 4));
                }
                resp.push_str("0000");
                (200, "application/x-git-upload-pack-result", resp.into_bytes())
            } else if path.starts_with("/o/r/tar.gz/") {
                match codeload {
                    Codeload::Ok | Codeload::PartialLegacy | Codeload::ExpressSite | Codeload::ExpressSiteRest => (200, "application/gzip", archive.clone()),
                    Codeload::Private | Codeload::PrivateWithSite => (404, "text/plain", Vec::new()),
                    Codeload::Fail => (500, "text/plain", b"boom".to_vec()),
                    Codeload::MissingWithAuth if authed => (404, "text/plain", Vec::new()),
                    Codeload::MissingWithAuth | Codeload::BadToken => (200, "application/gzip", archive.clone()),
                    Codeload::Slow => {
                        std::thread::sleep(std::time::Duration::from_secs(8));
                        (200, "application/gzip", archive.clone())
                    }
                }
            } else if path.starts_with("/repos/") {
                if api_blocked {
                    (403, "application/json", br#"{"message":"API rate limit exceeded"}"#.to_vec())
                } else if codeload == Codeload::PartialLegacy && path.starts_with("/repos/reactjs/legacy.reactjs.org/git/trees/") {
                    json(serde_json::json!({"tree":[
                        {"path":"content/docs/good.md","type":"blob","sha":"b1","size":6},
                        {"path":"content/docs/bad.md","type":"blob","sha":"b2","size":6}]}))
                } else if express && path == "/repos/expressjs/expressjs.com/git/trees/main?recursive=1" {
                    let mut tree: Vec<_> = EXPRESS_SITE_DOCS
                        .iter()
                        .map(|(path, body)| serde_json::json!({"path": path, "type": "blob", "size": body.len()}))
                        .collect();
                    tree.push(serde_json::json!({"path": express_oversize, "type": "blob", "size": super::MAX_FILE + 1}));
                    json(serde_json::json!({"tree": tree}))
                } else if path.contains("/git/trees/docs-sha") {
                    json(serde_json::json!({"tree":[{"path":"guide.md","type":"blob","sha":"b2","size":7}]}))
                } else if path.contains("/git/trees/") {
                    json(serde_json::json!({"tree":[
                        {"path":"README.md","type":"blob","sha":"b1","size":8},
                        {"path":"docs","type":"tree","sha":"docs-sha","size":0},
                        {"path":"src","type":"tree","sha":"src-sha","size":0}]}))
                } else {
                    (404, "application/json", b"{}".to_vec())
                }
            } else if express && path == "/npm/express/latest" {
                json(serde_json::json!({"version":"5.3.0"}))
            } else if express && path == "/expressjs/expressjs.com/tar.gz/main" {
                if codeload == Codeload::ExpressSiteRest {
                    (500, "text/plain", Vec::new())
                } else {
                    (200, "application/gzip", express_archive.clone())
                }
            } else if let Some(p) = path.strip_prefix("/raw/expressjs/expressjs.com/main/").filter(|_| express) {
                match EXPRESS_SITE_DOCS.iter().find(|(path, _)| *path == p) {
                    Some((_, body)) => (200, "text/markdown", body.to_vec()),
                    None => (404, "text/plain", Vec::new()),
                }
            } else if path == "/npm/react/latest" {
                json(serde_json::json!({"version":"19.0.0"}))
            } else if path.starts_with("/reactjs/legacy.reactjs.org/tar.gz/") {
                if codeload == Codeload::PartialLegacy {
                    (500, "text/plain", b"boom".to_vec())
                } else {
                    (200, "application/gzip", legacy_archive.clone())
                }
            } else if path.starts_with("/reactjs/react.dev/tar.gz/") {
                if site_docs_present {
                    (200, "application/gzip", site_archive.clone())
                } else {
                    (404, "text/plain", Vec::new())
                }
            } else if path.starts_with("/raw/reactjs/legacy.reactjs.org/") && path.ends_with("/good.md") {
                (200, "text/markdown", b"# good".to_vec())
            } else if path.starts_with("/raw/o/r/") {
                (200, "text/markdown", b"# raw".to_vec())
            } else {
                (404, "text/plain", Vec::new())
            };
            let _ = write!(
                c,
                "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = c.write_all(&body);
            let _ = c.flush();
            let _ = c.shutdown(std::net::Shutdown::Write);
        }
    });
    Fake { base, seen }
}

pub(super) fn client(f: &Fake, max_archive: u64) -> Client {
    let agent = ureq::Agent::config_builder().http_status_as_error(false).proxy(None).build().into();
    Client {
        agent,
        authenticated: false,
        credentialed_sources: false,
        git_host: None,
        host_creds: Default::default(),
        deadline: None,
        hosts: Hosts {
            api: f.base.clone(),
            git: f.base.clone(),
            codeload: f.base.clone(),
            raw: format!("{}/raw", f.base),
            npm: format!("{}/npm", f.base),
        },
        max_archive,
    }
}

pub(super) fn authed_client(f: &Fake) -> Client {
    let mut c = client(f, MAX_ARCHIVE);
    c.authenticated = true;
    c
}

/// A client with the future credentialed-sources hook switched on.
pub(super) fn credentialed_client(f: &Fake) -> Client {
    let mut c = authed_client(f);
    c.credentialed_sources = true;
    c
}

static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn with_token<T>(f: impl FnOnce() -> T) -> T {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("GITHUB_TOKEN", "test-token");
    std::env::remove_var("GH_TOKEN");
    let r = f();
    std::env::remove_var("GITHUB_TOKEN");
    r
}

pub(super) fn authed_requests(f: &Fake) -> usize {
    f.seen.lock().unwrap().iter().filter(|l| l.ends_with(" AUTH")).count()
}

pub(super) fn run_build(f: &Fake, max_archive: u64) -> (Result<Manifest>, tempfile::TempDir) {
    let out = tempfile::tempdir().unwrap();
    let r = build(&client(f, max_archive), &dep("pkg", "1.2.3"), &repo(None), false, out.path());
    (r, out)
}

pub(super) fn requests(f: &Fake, prefix: &str) -> usize {
    f.seen
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.split(' ').nth(1).unwrap_or("").starts_with(prefix))
        .count()
}

/// Paths that received an Authorization header.
pub(super) fn authed_paths(f: &Fake) -> Vec<String> {
    f.seen
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.ends_with(" AUTH"))
        .map(|l| l.split(' ').nth(1).unwrap_or("").to_string())
        .collect()
}

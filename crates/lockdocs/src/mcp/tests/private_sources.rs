use super::{app, call, text, token, write, TEST_ENV};
use crate::pro;
use ed25519_dalek::SigningKey;
use mcp_kit::licence::LicencePolicy;
use serde_json::json;
use std::path::Path;

// Called by the test that owns the process-global cache and licence settings.
type Seen = std::sync::Arc<std::sync::Mutex<Vec<(String, Option<String>)>>>;

/// A one-file HTTP server: answers by path and records (path, Authorization).
fn serve(route: impl Fn(&str, Option<&str>) -> (u16, Vec<u8>) + Send + 'static) -> (String, Seen) {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let seen: Seen = Default::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut c) = s else { return };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 2048];
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
            let (status, body) = route(&path, auth.as_deref());
            let _ = write!(c, "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = c.write_all(&body);
            let _ = c.shutdown(std::net::Shutdown::Write);
        }
    });
    (base, seen)
}

pub(super) fn check(tmp: &Path, policy: LicencePolicy<'static>, key: &SigningKey) {
    std::env::set_var("LOCKDOCS_NO_UPSTREAM", "1");
    std::env::set_var("HOME", tmp.join("home"));
    let tarball = {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()));
        let data = b"# Acme UI\n\nHello private world.\n";
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        tar.append_data(&mut h, "package/README.md", &data[..]).unwrap();
        tar.into_inner().unwrap().finish().unwrap()
    };
    let (files, files_seen) = serve(move |_, _| (200, tarball.clone()));
    let (reg, reg_seen) = serve(move |p, auth| match (p, auth) {
        ("/@acme%2Fui/1.0.0", Some("Bearer SECRET-MCP")) => (200, format!(r#"{{"dist":{{"tarball":"{files}/ui.tgz"}}}}"#).into_bytes()),
        _ => (401, Vec::new()),
    });
    let proj = tmp.join("private-proj");
    write(&proj.join("package.json"), r#"{"name":"app","dependencies":{"@acme/ui":"^1.0.0"}}"#);
    write(
        &proj.join("package-lock.json"),
        r#"{"name":"app","lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"@acme/ui":"^1.0.0"}},"node_modules/@acme/ui":{"version":"1.0.0"}}}"#,
    );
    write(
        &proj.join(".npmrc"),
        &format!("@acme:registry={reg}/\n//{}/:_authToken=SECRET-MCP\n", reg.trim_start_matches("http://")),
    );
    let args = json!({"query": "hello", "package": "@acme/ui", "root": proj.to_str().unwrap()});

    // Without Pro: the pro_required answer, and nothing was sent to the private host.
    std::env::remove_var(TEST_ENV);
    let mut a = app(policy);
    a.opts.fetch = true;
    let r = call(&a, "docs", args.clone());
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["pro_required"]["feature"], pro::PRIVATE_SOURCES, "{r}");
    assert!(!text(&r).contains("SECRET"));
    assert!(
        reg_seen.lock().unwrap().is_empty() && files_seen.lock().unwrap().is_empty(),
        "a request left the machine without Pro"
    );
    // The same call in a project with no private config is not gated.
    let free = call(
        &a,
        "docs",
        json!({"query": "object schema", "package": "tiny-schema", "root": tmp.join("proj").to_str().unwrap()}),
    );
    assert!(free["structuredContent"].is_null(), "{free}");

    // With Pro: the docs come from the private registry; the token reached only that origin.
    std::env::set_var(TEST_ENV, token(key, r#"{"plan":"pro","issuedAt":1,"product":"lockdocs","seats":1}"#));
    let r = call(&a, "docs", args);
    assert!(r["structuredContent"].is_null(), "{r}");
    assert!(text(&r).contains("Hello private world"), "{}", text(&r));
    assert!(!text(&r).contains("SECRET"));
    assert!(reg_seen.lock().unwrap().iter().all(|(_, a)| a.as_deref() == Some("Bearer SECRET-MCP")));
    assert!(
        files_seen.lock().unwrap().iter().all(|(_, a)| a.is_none()),
        "the token followed the tarball URL to another origin"
    );
    std::env::remove_var(TEST_ENV);
}

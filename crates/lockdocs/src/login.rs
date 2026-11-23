//! Native client for the browser consent / verifier-bound code exchange.
//! The web service owns consent, code encryption, and account licence lookup.

use crate::pro::{POLICY, TEST_POLICY};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use mcp_kit::licence::{self, LicencePolicy};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::env::{self, VarError};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use url::Url;

// The production LOCKDOCS_APP_ORIGIN declared in sylphx.toml.
const PRODUCTION_WEB_ORIGIN: &str = "https://lockdocs.sylphx.com";
const CALLBACK_WAIT: Duration = Duration::from_secs(300);
const REQUEST_WAIT: Duration = Duration::from_secs(2);
const EXCHANGE_WAIT: Duration = Duration::from_secs(20);
const MAX_HTTP_REQUEST: usize = 16 * 1024;
const MAX_EXCHANGE_BODY: u64 = 16 * 1024;

pub fn run(test: bool, no_browser: bool) -> Result<()> {
    let origin = login_origin(test)?;
    let policy = if test { &TEST_POLICY } else { &POLICY };
    let path = policy.token_path().context("no config directory on this machine")?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("cannot listen for the sign-in callback")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();

    let mut random = [0u8; 64];
    getrandom::fill(&mut random).map_err(|_| anyhow!("cannot generate secure sign-in randomness"))?;
    let (verifier_bytes, state_bytes) = random.split_at(32);
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let state = URL_SAFE_NO_PAD.encode(state_bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier_bytes));

    let mut url = origin.join("/cli/authorize")?;
    url.query_pairs_mut()
        .append_pair("port", &port.to_string())
        .append_pair("state", &state)
        .append_pair("challenge", &challenge);
    {
        let mut out = io::stdout().lock();
        writeln!(out, "Open this URL in your browser to sign in:\n{url}")?;
        out.flush()?;
    }
    if !no_browser {
        open_browser(&url);
    }

    // Only one well-formed, matching callback can reach the exchange endpoint.
    let code = wait_for_code(listener, port, &state)?;
    let grant = exchange(&origin, &code, &state, &verifier)?;
    save_grant(policy, &path, &grant)?;
    println!("{} {} activated ({})", policy.product, policy.tier, path.display());
    Ok(())
}

fn login_origin(test: bool) -> Result<Url> {
    match env::var("LOCKDOCS_WEB_URL") {
        Ok(origin) => web_origin(&origin),
        Err(VarError::NotPresent) if test => bail!("set LOCKDOCS_WEB_URL to a test service origin when using --test"),
        Err(VarError::NotPresent) => web_origin(PRODUCTION_WEB_ORIGIN),
        Err(VarError::NotUnicode(_)) => bail!("LOCKDOCS_WEB_URL must be valid Unicode"),
    }
}

fn web_origin(input: &str) -> Result<Url> {
    let invalid = || anyhow!("LOCKDOCS_WEB_URL must be an HTTPS origin (http://127.0.0.1 is allowed for local testing)");
    let (scheme, rest) = input.split_once("://").ok_or_else(invalid)?;
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    // Reject paths and userinfo before the URL parser can normalize them away.
    if authority.is_empty()
        || authority.ends_with(':')
        || authority.contains(['/', '\\', '?', '#', '@', '%'])
        || authority.chars().any(|c| c.is_whitespace() || c.is_control())
        || !(scheme == "https" || (scheme == "http" && authority.split(':').next() == Some("127.0.0.1")))
    {
        return Err(invalid());
    }
    let origin = Url::parse(input).map_err(|_| invalid())?;
    if !origin.has_host() || origin.port() == Some(0) {
        return Err(invalid());
    }
    Ok(origin)
}

fn open_browser(url: &Url) {
    let mut command = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        Command::new("xdg-open")
    };
    if let Ok(mut child) = command
        .arg(url.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

fn wait_for_code(listener: TcpListener, port: u16, state: &str) -> Result<String> {
    let deadline = Instant::now() + CALLBACK_WAIT;
    let host = format!("127.0.0.1:{port}");
    while Instant::now() < deadline {
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e).context("cannot accept the sign-in callback"),
        };
        // One slow or malformed request must not extend the authorization window.
        let request_deadline = deadline.min(Instant::now() + REQUEST_WAIT);
        if stream.set_nonblocking(false).is_err() || stream.set_write_timeout(Some(REQUEST_WAIT)).is_err() {
            continue;
        }
        let code = read_callback(&mut stream, request_deadline, &host, state);
        let (status, body) = if code.is_some() {
            ("200 OK", "Callback received. Return to your lockdocs terminal to see the result.\n")
        } else {
            ("400 Bad Request", "Invalid lockdocs sign-in callback.\n")
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        if let Some(code) = code {
            return Ok(code);
        }
    }
    bail!("timed out waiting for browser consent; run `lockdocs login` to try again")
}

fn read_callback(stream: &mut TcpStream, deadline: Instant, host: &str, state: &str) -> Option<String> {
    let mut buffer = [0u8; MAX_HTTP_REQUEST];
    let mut end = 0;
    while end < buffer.len() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        stream.set_read_timeout(Some(remaining)).ok()?;
        let n = stream.read(&mut buffer[end..]).ok()?;
        if n == 0 {
            break;
        }
        end += n;
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut request = httparse::Request::new(&mut headers);
        if let httparse::Status::Complete(header_end) = request.parse(&buffer[..end]).ok()? {
            if request.method != Some("GET") || header_end != end {
                return None;
            }
            let mut hosts = request.headers.iter().filter(|h| h.name.eq_ignore_ascii_case("host"));
            if hosts.next()?.value != host.as_bytes() || hosts.next().is_some() {
                return None;
            }
            if request
                .headers
                .iter()
                .any(|h| h.name.eq_ignore_ascii_case("transfer-encoding") || (h.name.eq_ignore_ascii_case("content-length") && h.value != b"0"))
            {
                return None;
            }
            return callback_code(request.path?, state);
        }
    }
    None
}

fn callback_code(target: &str, expected_state: &str) -> Option<String> {
    let query = target.strip_prefix("/callback?")?;
    if query.contains('#') {
        return None;
    }
    let mut state = None;
    let mut code = None;
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match name.as_ref() {
            "state" if state.is_none() => state = Some(value),
            "code" if code.is_none() && !value.is_empty() && !value.chars().any(char::is_control) => code = Some(value.into_owned()),
            // Never accept a grant or any other credential from the browser URL.
            _ => return None,
        }
    }
    if !bool::from(state?.as_bytes().ct_eq(expected_state.as_bytes())) {
        return None;
    }
    code
}

#[derive(Deserialize)]
struct ExchangeResponse {
    grant: String,
}

fn exchange(origin: &Url, code: &str, state: &str, verifier: &str) -> Result<String> {
    let url = origin.join("/api/cli/exchange")?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(origin.scheme() == "https")
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(EXCHANGE_WAIT))
        .build()
        .into();
    let payload = serde_json::to_vec(&serde_json::json!({ "code": code, "state": state, "verifier": verifier }))?;
    let mut response = agent
        .post(url.as_str())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send(payload.as_slice())
        .map_err(|_| anyhow!("cannot reach the sign-in exchange endpoint"))?;
    let status = response.status();
    if !status.is_success() {
        bail!(
            "sign-in exchange failed (HTTP {}); the service must return an account-bound lockdocs licence",
            status.as_u16()
        );
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_EXCHANGE_BODY)
        .read_to_vec()
        .map_err(|_| anyhow!("cannot read the sign-in exchange response"))?;
    let response: ExchangeResponse = serde_json::from_slice(&body).map_err(|_| anyhow!("sign-in exchange did not return a licence grant"))?;
    Ok(response.grant)
}

fn save_grant(policy: &LicencePolicy<'_>, path: &Path, grant: &str) -> Result<()> {
    let grant = grant.trim();
    policy.verify(grant).context("sign-in did not return a valid lockdocs licence")?;
    licence::write_token(path, grant).context("cannot save the verified licence")
}

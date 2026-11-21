//! Resolve release tags and branches through Git smart HTTP.

use std::io::Read;

use anyhow::{bail, Context, Result};

use crate::{Dep, Eco};

use super::client::Client;
use super::Repo;

/// A ref advertised by `git ls-remote`: full name, object id, and the commit
/// an annotated tag peels to.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct GitRef {
    pub(super) name: String,
    pub(super) oid: String,
    pub(super) peeled: Option<String>,
}

fn pkt_line(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(format!("{:04x}", s.len() + 4).as_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Body of a protocol-v2 `ls-refs` command limited to `prefixes`.
pub(super) fn ls_refs_request(prefixes: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    pkt_line(&mut out, "command=ls-refs\n");
    out.extend_from_slice(b"0001");
    pkt_line(&mut out, "peel\n");
    for p in prefixes {
        pkt_line(&mut out, &format!("ref-prefix {p}\n"));
    }
    out.extend_from_slice(b"0000");
    out
}

/// Parse an `ls-refs` response (pkt-lines: `<oid> <name>[ peeled:<oid>]`).
pub(super) fn parse_ls_refs(mut b: &[u8]) -> Result<Vec<GitRef>> {
    let mut refs = Vec::new();
    while b.len() >= 4 {
        let len = std::str::from_utf8(&b[..4])
            .ok()
            .and_then(|h| usize::from_str_radix(h, 16).ok())
            .context("malformed git response")?;
        if len < 4 {
            // flush (0000), delimiter (0001), response end (0002)
            b = &b[4..];
            if len == 0 {
                break;
            }
            continue;
        }
        if b.len() < len {
            bail!("truncated git response");
        }
        let line = String::from_utf8_lossy(&b[4..len]);
        b = &b[len..];
        let line = line.trim_end_matches('\n');
        if let Some(e) = line.strip_prefix("ERR ") {
            bail!("git server error: {e}");
        }
        let mut parts = line.split(' ');
        let (Some(oid), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        let peeled = parts.find_map(|a| a.strip_prefix("peeled:")).map(String::from);
        refs.push(GitRef {
            name: name.to_string(),
            oid: oid.to_string(),
            peeled,
        });
    }
    Ok(refs)
}

fn is_object_id(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(super) fn tag_prefixes(candidates: &[String]) -> Vec<String> {
    candidates.iter().map(|t| format!("refs/tags/{t}")).collect()
}

/// The first candidate tag that exists exactly, with the immutable commit it
/// points to (annotated tags are peeled by the server).
pub(super) fn pick_release(candidates: &[String], refs: &[GitRef]) -> Option<(String, String)> {
    candidates.iter().find_map(|t| {
        let name = format!("refs/tags/{t}");
        let r = refs.iter().find(|r| r.name == name)?;
        let sha = r.peeled.as_deref().unwrap_or(&r.oid);
        is_object_id(sha).then(|| (t.clone(), sha.to_string()))
    })
}

/// Refs under `prefixes` of a repository over the git smart-HTTP protocol (what
/// `git ls-remote` does). It is not governed by the REST API quota. Ok(None)
/// when the repository does not exist or is private.
pub(super) fn list_refs(agent: &Client, repo: &Repo, prefixes: &[String]) -> Result<Option<Vec<GitRef>>> {
    agent.check_deadline()?;
    let url = format!("{}/{}/{}.git/git-upload-pack", agent.hosts.git, repo.owner, repo.name);
    let sent = agent.source_header(&url, true);
    let mut req = agent
        .post(&url)
        .header("Content-Type", "application/x-git-upload-pack-request")
        .header("Accept", "application/x-git-upload-pack-result")
        .header("Git-Protocol", "version=2");
    let sent_token = match sent {
        Some(h) => {
            req = req.header("Authorization", &h);
            true
        }
        None => false,
    };
    let mut res = req.send(&ls_refs_request(prefixes)[..])?;
    let status = res.status().as_u16();
    if status == 401 && sent_token {
        if agent.git_host.is_some() {
            bail!(
                "{} rejected the configured credential for {}/{} (HTTP 401); refresh it",
                agent.hosts.git,
                repo.owner,
                repo.name
            );
        }
        bail!(
            "GitHub rejected GITHUB_TOKEN/GH_TOKEN for {}/{} (HTTP 401); refresh or unset it",
            repo.owner,
            repo.name
        );
    }
    if status == 404 || status == 401 {
        return Ok(None);
    }
    if status >= 300 {
        bail!("git refs for {}/{}: HTTP {status}", repo.owner, repo.name);
    }
    let mut body = Vec::new();
    res.body_mut().as_reader().take(64 << 20).read_to_end(&mut body)?;
    Ok(Some(parse_ls_refs(&body)?))
}

/// Tag names projects use for a release.
pub fn tag_candidates(dep: &Dep) -> Vec<String> {
    let v = dep.version.trim_start_matches('v');
    let short = dep.name.rsplit('/').next().unwrap_or(&dep.name);
    let mut c = vec![
        format!("v{v}"),
        v.to_string(),
        format!("{}@{v}", dep.name),
        format!("{short}@{v}"),
        format!("{short}-v{v}"),
        format!("{short}-{v}"),
        format!("rel_{}", v.replace('.', "_")),
        format!("release-{v}"),
    ];
    if dep.eco == Eco::Go {
        c.insert(0, dep.version.clone());
    }
    c.dedup();
    c
}

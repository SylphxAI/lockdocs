//! Read a GitHub tree and download individual files when an archive cannot be used.

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Result};
use rayon::prelude::*;

use super::client::{api, enc, enc_path, Client};
use super::{Repo, MAX_FILE};

pub(super) struct Item {
    pub(super) path: String,
    pub(super) kind: String,
    pub(super) sha: String,
    pub(super) size: u64,
}

pub(super) fn tree(agent: &Client, repo: &Repo, sha_or_ref: &str, recursive: bool) -> Result<Option<Vec<Item>>> {
    let q = if recursive { "?recursive=1" } else { "" };
    let Some(v) = api(agent, &format!("/repos/{}/{}/git/trees/{}{q}", repo.owner, repo.name, enc(sha_or_ref)))? else {
        return Ok(None);
    };
    let items = v
        .get("tree")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .map(|i| Item {
                    path: i.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                    kind: i.get("type").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                    sha: i.get("sha").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                    size: i.get("size").and_then(|p| p.as_u64()).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(items))
}

pub(super) fn download(agent: &Client, repo: &Repo, reference: &str, files: &[(String, u64)], out: &Path) -> Result<Vec<u64>> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(16).build()?;
    let results: Vec<Result<u64>> = pool.install(|| {
        files
            .par_iter()
            .map(|(p, _)| {
                let url = format!("{}/{}/{}/{}/{}", agent.hosts.raw, repo.owner, repo.name, enc(reference), enc_path(p));
                agent.check_deadline()?;
                let mut res = agent.get(&url).call()?;
                if res.status().as_u16() != 200 {
                    bail!("HTTP {} for {p}", res.status());
                }
                let mut buf = Vec::new();
                res.body_mut().as_reader().take(MAX_FILE + 1).read_to_end(&mut buf)?;
                if buf.len() as u64 > MAX_FILE {
                    bail!("upstream file too large: {p}");
                }
                let Some(rel) = crate::fetch::safe_rel(Path::new(p), false) else {
                    bail!("unsafe path {p}")
                };
                let dst = out.join(rel);
                if let Some(d) = dst.parent() {
                    std::fs::create_dir_all(d)?;
                }
                std::fs::write(dst, &buf)?;
                Ok(buf.len() as u64)
            })
            .collect()
    });
    download_results(results)
}

fn download_results(results: Vec<Result<u64>>) -> Result<Vec<u64>> {
    let failed = results.iter().filter(|r| r.is_err()).count();
    if failed > 0 {
        let first = results.iter().find_map(|r| r.as_ref().err()).expect("failed download");
        bail!("{failed}/{} upstream files failed to download: {first:#}", results.len());
    }
    results.into_iter().collect()
}

pub(super) fn write_file(root: &Path, p: &str, data: &[u8]) -> Result<()> {
    let Some(rel) = crate::fetch::safe_rel(Path::new(p), false) else {
        bail!("unsafe path {p}")
    };
    let dst = root.join(rel);
    if let Some(d) = dst.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(dst, data)?;
    Ok(())
}

//! Select and download docs from an exact package release.

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::Dep;

use super::archive::{codeload, read_tar};
use super::client::Client;
use super::github_files::{download, tree, write_file, Item};
use super::{Repo, MAX_BYTES, MAX_FILE, MAX_FILES};

/// Which files of a package repository to keep. A docs root is a `docs`-like
/// folder at the top, or one or two levels inside wrapper folders (monorepos
/// keep them there); README/CHANGELOG-like files sit at the top or in the
/// package's own folder.
struct PkgScan {
    short: String,
    sub_base: String,
    subdir: Option<String>,
    roots: std::collections::BTreeSet<String>,
    langs: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// (full path, docs root it sits in, path inside that root, is a README-like file)
    cands: Vec<(String, Option<String>, String, bool)>,
}

impl PkgScan {
    fn new(dep: &Dep, repo: &Repo) -> Self {
        PkgScan {
            short: dep.name.rsplit('/').next().unwrap_or(&dep.name).to_ascii_lowercase(),
            sub_base: repo.subdir.as_deref().and_then(|s| s.rsplit('/').next()).unwrap_or("").to_ascii_lowercase(),
            subdir: repo.subdir.clone(),
            roots: Default::default(),
            langs: Default::default(),
            cands: Vec::new(),
        }
    }

    /// Number of leading folders that form the docs root of `dirs`, if any.
    fn root_len(&self, dirs: &[&str]) -> Option<usize> {
        for (i, d) in dirs.iter().take(3).enumerate() {
            let name = d.to_ascii_lowercase();
            if DOC_ROOTS.contains(&name.as_str()) {
                return Some(i + 1);
            }
            // Only wanted folders are opened to look further down.
            let full = dirs[..=i].join("/");
            let wanted = WRAPPERS.contains(&name.as_str())
                || (i >= 1 && (name == self.short || name == self.sub_base || name.contains("docs")))
                || self.subdir.as_deref().is_some_and(|s| s == full || s.starts_with(&format!("{full}/")));
            if i >= 2 || !wanted {
                return None;
            }
        }
        None
    }

    /// Look at one archive path; true when its content may be wanted.
    fn observe(&mut self, path: &str, size: u64) -> bool {
        let comps: Vec<&str> = path.split('/').collect();
        let (file, dirs) = comps.split_last().expect("non-empty path");
        let root = self.root_len(dirs).map(|n| dirs[..n].join("/"));
        let mut rel_in_root = None;
        if let Some(r) = &root {
            self.roots.insert(r.clone());
            let rel = &comps[r.split('/').count()..];
            if rel.len() > 1 {
                self.langs.entry(r.clone()).or_default().insert(rel[0].to_string());
            }
            rel_in_root = Some(rel.join("/"));
        }
        let upper = file.to_ascii_uppercase();
        let near = dirs.is_empty() || self.subdir.as_deref() == Some(dirs.join("/").as_str());
        let readme = near
            && doc_file(file)
            && ["README", "CHANGELOG", "CHANGES", "HISTORY", "MIGRAT", "UPGRAD", "RELEASE"]
                .iter()
                .any(|p| upper.starts_with(p));
        let in_root = rel_in_root.as_deref().is_some_and(doc_file);
        let wanted = size <= MAX_FILE && (readme || in_root);
        if wanted {
            self.cands
                .push((path.to_string(), root.filter(|_| in_root), rel_in_root.unwrap_or_default(), readme));
        }
        wanted
    }

    /// Apply the root, language and size limits to what was read.
    fn select(self, mut data: std::collections::HashMap<String, Vec<u8>>) -> Vec<(String, Vec<u8>)> {
        let chosen: Vec<&String> = self.roots.iter().take(4).collect();
        let mut keep: Vec<String> = Vec::new();
        for (full, root, rel, readme) in &self.cands {
            let in_chosen = root.as_ref().is_some_and(|r| chosen.contains(&r));
            if !in_chosen && !readme {
                continue;
            }
            if in_chosen && !readme {
                let langs = self.langs.get(root.as_ref().expect("root"));
                let only_en = langs.is_some_and(|l| l.iter().filter(|x| is_lang(x)).count() >= 2 && l.contains("en"));
                let first = rel.split('/').next().unwrap_or("");
                if only_en && rel.contains('/') && is_lang(first) && first != "en" {
                    continue;
                }
            }
            keep.push(full.clone());
        }
        keep.sort();
        keep.dedup();
        let mut total = 0u64;
        let mut out = Vec::new();
        for p in keep {
            let Some(d) = data.remove(&p) else { continue };
            if total.saturating_add(d.len() as u64) > MAX_BYTES {
                continue;
            }
            total += d.len() as u64;
            out.push((p, d));
            if out.len() >= MAX_FILES {
                break;
            }
        }
        out
    }
}

/// The per-file fallback for one repository: GitHub's REST tree walk plus raw
/// file requests (subject to the REST quota). Returns files written and bytes.
pub(super) fn rest_docs(agent: &Client, dep: &Dep, repo: &Repo, commit: &str, out: &Path) -> Result<(usize, u64)> {
    let root = tree(agent, repo, commit, false)?.context("release commit tree not found")?;
    let short = dep.name.rsplit('/').next().unwrap_or(&dep.name).to_ascii_lowercase();
    let sub_base = repo.subdir.as_deref().and_then(|s| s.rsplit('/').next()).unwrap_or("").to_ascii_lowercase();
    let mut roots: Vec<(String, String)> = Vec::new(); // (path, tree sha)
    let mut files: Vec<(String, u64)> = Vec::new();
    let mut calls = 0;
    let mut frontier: Vec<(String, Vec<Item>, usize)> = vec![(String::new(), root, 0)];
    while let Some((prefix, items, depth)) = frontier.pop() {
        for it in items {
            let name = it.path.to_ascii_lowercase();
            let full = if prefix.is_empty() {
                it.path.clone()
            } else {
                format!("{prefix}/{}", it.path)
            };
            if it.kind == "blob" {
                // Root-level (or package-level) READMEs and changelogs.
                let upper = it.path.to_ascii_uppercase();
                let near = depth == 0 || repo.subdir.as_deref() == Some(prefix.as_str());
                if near
                    && doc_file(&it.path)
                    && ["README", "CHANGELOG", "CHANGES", "HISTORY", "MIGRAT", "UPGRAD", "RELEASE"]
                        .iter()
                        .any(|p| upper.starts_with(p))
                {
                    files.push((full, it.size));
                }
                continue;
            }
            if it.kind != "tree" {
                continue;
            }
            if DOC_ROOTS.contains(&name.as_str()) {
                roots.push((full, it.sha));
            } else if depth < 2 && calls < 10 {
                let wanted = WRAPPERS.contains(&name.as_str())
                    || (depth >= 1 && (name == short || name == sub_base || name.contains("docs")))
                    || repo.subdir.as_deref().is_some_and(|s| s == full || s.starts_with(&format!("{full}/")));
                if wanted {
                    calls += 1;
                    if let Some(children) = tree(agent, repo, &it.sha, false)? {
                        frontier.push((full, children, depth + 1));
                    }
                }
            }
        }
    }
    for (path, sha) in roots.iter().take(4) {
        let Some(items) = tree(agent, repo, sha, true)? else { continue };
        // Keep English when the docs are split by language.
        let langs: Vec<&str> = items
            .iter()
            .filter(|i| i.kind == "tree" && !i.path.contains('/') && is_lang(&i.path))
            .map(|i| i.path.as_str())
            .collect();
        let only_en = langs.len() >= 2 && langs.contains(&"en");
        for i in &items {
            if i.kind != "blob" || !doc_file(&i.path) || i.size > MAX_FILE {
                continue;
            }
            if only_en {
                let first = i.path.split('/').next().unwrap_or("");
                if is_lang(first) && first != "en" {
                    continue;
                }
            }
            files.push((format!("{path}/{}", i.path), i.size));
        }
    }
    files.sort();
    files.dedup();
    let mut total = 0u64;
    files.retain(|(_, s)| {
        if *s > MAX_FILE || total.saturating_add(*s) > MAX_BYTES {
            return false;
        }
        total += s;
        true
    });
    files.truncate(MAX_FILES);
    let ok = download(agent, repo, commit, &files, out)?;
    Ok((ok.len(), ok.iter().sum()))
}

/// Download one repository snapshot and keep its docs (see `PkgScan`).
pub(super) fn extract_pkg_docs(body: impl Read, dep: &Dep, repo: &Repo) -> Result<Vec<(String, Vec<u8>)>> {
    let mut scan = PkgScan::new(dep, repo);
    let data = read_tar(body, MAX_BYTES * 2, |p, size| scan.observe(p, size))?;
    Ok(scan.select(data.into_iter().collect()))
}

const DOC_ROOTS: &[&str] = &["docs", "doc", "documentation", "guide", "guides", "docs_src"];
const WRAPPERS: &[&str] = &["website", "site", "www", "apps", "packages", "content", "src"];

pub(super) fn doc_file(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    let example = (l.contains("example") || l.contains("snippet") || l.starts_with("docs_src/"))
        && [".py", ".ts", ".tsx", ".js", ".jsx", ".rs", ".go"].iter().any(|e| l.ends_with(e));
    // Django and others write reStructuredText in .txt files under docs/.
    let ext_ok = example || [".md", ".mdx", ".rst", ".markdown", ".mdoc", ".txt"].iter().any(|e| l.ends_with(e));
    let skip = [
        "/blog/",
        "/node_modules/",
        "/i18n/",
        "/_snippets/",
        "/images/",
        "/public/",
        "/.github/",
        "/test/",
        "/tests/",
    ]
    .iter()
    .any(|s| format!("/{l}").contains(s));
    ext_ok && !skip
}

fn is_lang(s: &str) -> bool {
    (s.len() == 2 && s.chars().all(|c| c.is_ascii_lowercase())) || (s.len() == 5 && s.as_bytes()[2] == b'-')
}

/// Stream the release archive and write its docs: (files, bytes, "codeload").
/// Errors when the archive is missing, over the cap, or unreadable.
pub(super) fn codeload_docs(agent: &Client, dep: &Dep, repo: &Repo, tag: &str, commit: &str, out: &Path) -> Result<(usize, u64, &'static str)> {
    let Some(body) = codeload(agent, repo, commit)? else {
        bail!("tag {tag} points to commit {commit}, but codeload.github.com has no archive for it (HTTP 404)");
    };
    let read = extract_pkg_docs(body.body, dep, repo);
    let docs = match read {
        Ok(d) => d,
        Err(_) if body.exceeded.load(std::sync::atomic::Ordering::Relaxed) => {
            bail!("archive is over the {} MB download cap", agent.max_archive >> 20)
        }
        Err(e) => return Err(e.context(format!("reading the archive of {} at {tag}", repo.name))),
    };
    let mut bytes = 0u64;
    for (p, d) in &docs {
        write_file(out, p, d)?;
        bytes += d.len() as u64;
    }
    Ok((docs.len(), bytes, "codeload"))
}

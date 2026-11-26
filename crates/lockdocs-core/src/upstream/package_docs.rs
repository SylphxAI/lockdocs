//! Select and download docs from an exact package release.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::Dep;

use super::archive::{codeload, read_tar};
use super::client::Client;
use super::github_files::{download, tree, write_file};
use super::{Repo, MAX_BYTES, MAX_FILE, MAX_FILES};

/// Which files of a package repository to keep. A docs root is a `docs`-like
/// folder at the top, or one or two levels inside wrapper folders (monorepos
/// keep them there); README/CHANGELOG-like files sit at the top or in the
/// package's own folder.
struct PkgScan {
    short: String,
    sub_base: String,
    subdir: Option<String>,
    roots: BTreeSet<String>,
    langs: BTreeMap<String, BTreeSet<String>>,
    /// Full path -> (docs root, first language segment, README-like, size).
    cands: BTreeMap<String, (Option<String>, Option<String>, bool, u64)>,
}

impl PkgScan {
    fn new(dep: &Dep, repo: &Repo) -> Self {
        PkgScan {
            short: dep.name.rsplit('/').next().unwrap_or(&dep.name).to_ascii_lowercase(),
            sub_base: repo.subdir.as_deref().and_then(|s| s.rsplit('/').next()).unwrap_or("").to_ascii_lowercase(),
            subdir: repo.subdir.clone(),
            roots: Default::default(),
            langs: Default::default(),
            cands: BTreeMap::new(),
        }
    }

    /// Number of leading folders that form the docs root of `dirs`, if any.
    fn root_len(&self, dirs: &[&str]) -> Option<usize> {
        for (i, d) in dirs.iter().take(3).enumerate() {
            let name = d.to_ascii_lowercase();
            if DOC_ROOTS.contains(&name.as_str()) {
                return Some(i + 1);
            }
            if !self.opens_wrapper(dirs, i) {
                return None;
            }
        }
        None
    }

    fn opens_wrapper(&self, dirs: &[&str], i: usize) -> bool {
        let name = dirs[i].to_ascii_lowercase();
        let full = dirs[..=i].join("/");
        i < 2
            && (WRAPPERS.contains(&name.as_str())
                || (i >= 1 && (name == self.short || name == self.sub_base || name.contains("docs")))
                || self.subdir.as_deref().is_some_and(|s| s == full || s.starts_with(&format!("{full}/"))))
    }

    /// Record one regular repository file; true if the archive should keep its bytes.
    fn observe(&mut self, path: &str, size: u64) -> bool {
        if !safe_path(path) {
            return false;
        }
        let comps: Vec<&str> = path.split('/').collect();
        let (file, dirs) = comps.split_last().expect("non-empty path");
        let mut root = None;
        let mut language = None;
        if let Some(n) = self.root_len(dirs) {
            let r = dirs[..n].join("/");
            self.roots.insert(r.clone());
            if dirs.len() > n && is_lang(comps[n]) {
                self.langs.entry(r.clone()).or_default().insert(comps[n].into());
                language = Some(comps[n].into());
            }
            // Include the selected root itself: e.g. docs_src/events/tutorial001.py.
            if doc_file(&comps[n - 1..].join("/")) {
                root = Some(r);
            }
        }
        let upper = file.to_ascii_uppercase();
        let near = dirs.is_empty() || self.subdir.as_deref() == Some(dirs.join("/").as_str());
        let readme = near
            && doc_file(file)
            && ["README", "CHANGELOG", "CHANGES", "HISTORY", "MIGRAT", "UPGRAD", "RELEASE"]
                .iter()
                .any(|p| upper.starts_with(p));
        let wanted = size <= MAX_FILE && (readme || root.is_some());
        if wanted {
            self.cands.insert(path.into(), (root, language, readme, size));
        }
        wanted
    }

    /// Apply identical root, language, and materialization budgets to both transports.
    fn select(self, available: impl Fn(&str) -> bool) -> Vec<(String, u64)> {
        let chosen: BTreeSet<_> = self.roots.iter().take(4).collect();
        let mut bytes = 0;
        self.cands
            .into_iter()
            .filter_map(|(path, (root, language, readme, size))| {
                if !readme {
                    let r = root.as_ref()?;
                    if !chosen.contains(r) {
                        return None;
                    }
                    let english_only = self.langs.get(r).is_some_and(|l| l.len() >= 2 && l.contains("en"));
                    if english_only && language.is_some_and(|l| l != "en") {
                        return None;
                    }
                }
                if !available(&path) || bytes + size > MAX_BYTES {
                    return None;
                }
                bytes += size;
                Some((path, size))
            })
            .take(MAX_FILES)
            .collect()
    }
}

fn safe_path(path: &str) -> bool {
    !path.contains(['\\', ':', '\0']) && path.split('/').all(|c| !matches!(c, "" | "." | ".."))
}

/// The per-file fallback: discover the same roots via GitHub's REST trees,
/// then fetch selected regular files only, always at the resolved commit.
pub(super) fn rest_docs(agent: &Client, dep: &Dep, repo: &Repo, commit: &str, out: &Path) -> Result<(usize, u64)> {
    let root = tree(agent, repo, commit, false)?.context("release commit tree not found")?;
    let mut scan = PkgScan::new(dep, repo);
    let mut roots = BTreeMap::new();
    let mut calls = 0;
    let mut frontier = vec![(String::new(), root)];
    while let Some((prefix, items)) = frontier.pop() {
        for it in items {
            if it.path.contains('/') || !safe_path(&it.path) {
                continue;
            }
            let full = if prefix.is_empty() {
                it.path.clone()
            } else {
                format!("{prefix}/{}", it.path)
            };
            if it.is_regular() {
                scan.observe(&full, it.size);
            } else if it.kind == "tree" {
                let dirs: Vec<_> = full.split('/').collect();
                if scan.root_len(&dirs).is_some() {
                    roots.insert(full, it.sha);
                } else if calls < 10 && scan.opens_wrapper(&dirs, dirs.len() - 1) {
                    calls += 1;
                    if let Some(children) = tree(agent, repo, &it.sha, false)? {
                        frontier.push((full, children));
                    }
                }
            }
        }
    }
    for (path, sha) in roots.into_iter().take(4) {
        let Some(items) = tree(agent, repo, &sha, true)? else { continue };
        for i in items.into_iter().filter(|i| i.is_regular()) {
            scan.observe(&format!("{path}/{}", i.path), i.size);
        }
    }
    let files = scan.select(|_| true);
    let ok = download(agent, repo, commit, &files, out)?;
    let bytes = ok.iter().sum();
    if bytes > MAX_BYTES {
        bail!("release docs are over the {} MB limit", MAX_BYTES >> 20);
    }
    Ok((ok.len(), bytes))
}

/// Download one repository snapshot and keep its docs (see `PkgScan`).
pub(super) fn extract_pkg_docs(body: impl Read, dep: &Dep, repo: &Repo) -> Result<Vec<(String, Vec<u8>)>> {
    let mut scan = PkgScan::new(dep, repo);
    let mut data: std::collections::HashMap<_, _> = read_tar(body, MAX_BYTES * 2, |p, size| scan.observe(p, size))?.into_iter().collect();
    Ok(scan
        .select(|p| data.contains_key(p))
        .into_iter()
        .filter_map(|(p, _)| data.remove(&p).map(|d| (p, d)))
        .collect())
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

//! Docs-site mappings and major-version lookup.

use std::io::Read;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::upstream::client::{api, Client};
use crate::upstream::git_refs::{list_refs, pick_release, tag_candidates, tag_prefixes};
use crate::upstream::Repo;
use crate::{Dep, Eco};

/// Packages whose docs live in a separate website repository. Paths with
/// `{major}` are versioned and used for any major. The `current` folders
/// describe one major: the latest one on `branch`, an older one on a
/// `v{major}` / `{major}.x` branch or, when `before_next_major` is set, at the
/// last commit before the next major was released.
#[derive(Clone)]
pub(in crate::upstream) struct DocsSite {
    eco: Eco,
    names: &'static [&'static str],
    pub(in crate::upstream) repo: (&'static str, &'static str),
    pub(super) branch: &'static str,
    pub(in crate::upstream) versioned: &'static [&'static str],
    /// Docs folders; layouts move between majors, so every one that exists is used.
    pub(in crate::upstream) current: &'static [&'static str],
    /// Folders of docs pages written as JS/TSX components (installation steps).
    pub(in crate::upstream) pages: &'static [&'static str],
    /// Older majors may use the commit before the next major's release.
    /// Off for sites that document APIs before they ship (react.dev).
    pub(super) before_next_major: bool,
}

pub(in crate::upstream) const DOCS_SITES: &[DocsSite] = &[
    DocsSite {
        eco: Eco::Npm,
        names: &["react", "react-dom", "@types/react"],
        repo: ("reactjs", "react.dev"),
        branch: "main",
        versioned: &[],
        current: &["src/content/reference", "src/content/learn"],
        pages: &[],
        before_next_major: false,
    },
    DocsSite {
        eco: Eco::Npm,
        names: &["express", "@types/express"],
        repo: ("expressjs", "expressjs.com"),
        branch: "main",
        // Express keeps both majors on main. A broad English root would mix
        // majors, even for a project pinned to the latest major.
        versioned: &["src/content/api/{major}x", "src/content/docs/en/{major}x"],
        current: &[],
        pages: &[],
        before_next_major: false,
    },
    DocsSite {
        eco: Eco::Npm,
        names: &["tailwindcss"],
        repo: ("tailwindlabs", "tailwindcss.com"),
        branch: "main",
        versioned: &[],
        current: &["src/docs", "src/pages/docs"],
        pages: &["src/app/(docs)/docs/installation", "src/pages/docs/installation"],
        before_next_major: true,
    },
    DocsSite {
        eco: Eco::Npm,
        names: &["prisma", "@prisma/client"],
        repo: ("prisma", "docs"),
        branch: "main",
        versioned: &[],
        current: &["apps/docs/content/docs/orm", "content/200-orm"],
        pages: &[],
        before_next_major: true,
    },
    DocsSite {
        eco: Eco::Cargo,
        names: &["tokio"],
        repo: ("tokio-rs", "website"),
        branch: "master",
        versioned: &[],
        current: &["content/tokio/tutorial", "content/tokio/topics"],
        pages: &[],
        before_next_major: false,
    },
];

/// Resolve the mapped site, including older-major archive destinations. Fetch
/// and failure cleanup must agree on the repository directory they own.
pub(super) fn docs_site(dep: &Dep) -> Option<super::DocsSite> {
    let mut site = super::DOCS_SITES
        .iter()
        .find(|s| s.eco == dep.eco && s.names.contains(&dep.name.as_str()))?
        .clone();
    // react.dev includes APIs before release. Use the official frozen legacy
    // site for React 18 and its version archives for 16/17, never a date guess.
    if site.repo == ("reactjs", "react.dev") {
        let archive = match dep.version.split('.').next()? {
            "16" => Some("16.reactjs.org"),
            "17" => Some("17.reactjs.org"),
            "18" => Some("main"),
            _ => None,
        };
        if let Some(branch) = archive {
            site.repo = ("reactjs", "legacy.reactjs.org");
            site.branch = branch;
            site.current = &["content/docs"];
        }
    }
    Some(site)
}

/// The latest stable major of a package, from its registry.
pub(super) fn latest_major(agent: &Client, dep: &Dep) -> Result<Option<u64>> {
    let name = dep.name.strip_prefix("@types/").unwrap_or(&dep.name);
    let (url, pointer) = match dep.eco {
        Eco::Npm => (format!("{}/{}/latest", agent.hosts.npm, name.replace('/', "%2F")), "/version"),
        Eco::Cargo => (format!("https://crates.io/api/v1/crates/{name}"), "/crate/max_stable_version"),
        Eco::PyPI => (format!("https://pypi.org/pypi/{name}/json"), "/info/version"),
        Eco::Go => return Ok(None),
    };
    agent.check_deadline()?;
    let mut res = agent.get(&url).call().with_context(|| format!("latest-major lookup: {url}"))?;
    let status = res.status().as_u16();
    let mut body = String::new();
    res.body_mut()
        .as_reader()
        .take((16 << 20) + 1)
        .read_to_string(&mut body)
        .context("latest-major response read failed")?;
    if body.len() > 16 << 20 {
        bail!("latest-major response exceeds 16 MB");
    }
    super::latest_major_response(status, &body, pointer)
}

/// Only a genuine missing registry record is an empty lookup. Network, HTTP,
/// malformed JSON and invalid version responses must leave enrichment retryable.
pub(in crate::upstream) fn latest_major_response(status: u16, body: &str, pointer: &str) -> Result<Option<u64>> {
    if status == 404 {
        return Ok(None);
    }
    if status != 200 {
        bail!("latest-major registry lookup failed: HTTP {status}");
    }
    let v: Value = serde_json::from_str(body).context("invalid latest-major registry JSON")?;
    let version = v.pointer(pointer).and_then(Value::as_str).context("latest-major response missing version")?;
    let major = version
        .split('.')
        .next()
        .context("latest-major response has empty version")?
        .parse()
        .context("latest-major response has invalid version")?;
    Ok(Some(major))
}

/// When the package's next major was released: the commit date of its
/// `{major+1}.0.0` tag in the package repository.
pub(super) fn next_major_date(agent: &Client, dep: &Dep, repo: &Repo, major: u64) -> Result<Option<String>> {
    let next = Dep {
        version: format!("{}.0.0", major + 1),
        ..dep.clone()
    };
    let candidates = tag_candidates(&next);
    let Some(refs) = list_refs(agent, repo, &tag_prefixes(&candidates))? else {
        return Ok(None);
    };
    let Some((_, sha)) = pick_release(&candidates, &refs) else {
        return Ok(None);
    };
    // The one place that still needs a commit date, which only the REST API gives.
    let Some(v) = api(agent, &format!("/repos/{}/{}/commits/{sha}", repo.owner, repo.name))? else {
        return Ok(None);
    };
    Ok(v.pointer("/commit/committer/date").and_then(|d| d.as_str()).map(String::from))
}

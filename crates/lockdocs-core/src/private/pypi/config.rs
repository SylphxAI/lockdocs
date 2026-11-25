use crate::private::{netrc_for, read, Auth, Candidate, Credential, Creds, Env, Plan, Url};
use crate::{Dep, Eco};
use std::path::{Path, PathBuf};

const PUBLIC_HOSTS: &[&str] = &["pypi.org", "files.pythonhosted.org", "pypi.python.org"];
const DEFAULT: &str = "https://pypi.org/simple";

fn is_public(url: &str) -> bool {
    Url::parse(url).is_none_or(|u| PUBLIC_HOSTS.contains(&u.host.as_str()))
}

/// One configured index.
#[derive(Debug, Clone)]
struct Index {
    name: Option<String>,
    url: String,
    explicit: bool,
}

fn words(v: &str) -> impl Iterator<Item = &str> {
    v.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty())
}

/// `[global]`/`[install]` `index-url` and `extra-index-url` of a pip config.
pub(super) fn pip_conf(text: &str) -> (Option<String>, Vec<String>) {
    let (mut main, mut extra) = (None, Vec::new());
    let mut section = String::new();
    let mut key = String::new();
    for line in text.lines() {
        let raw = line.trim_end();
        if raw.trim().is_empty() || raw.trim_start().starts_with(['#', ';']) {
            continue;
        }
        if let Some(s) = raw.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = s.to_ascii_lowercase();
            key.clear();
            continue;
        }
        if !matches!(section.as_str(), "global" | "install") {
            continue;
        }
        let continuation = raw.starts_with([' ', '\t']);
        let value = if continuation && !key.is_empty() {
            raw.trim().to_string()
        } else if let Some((k, v)) = raw.split_once(['=', ':']) {
            key = k.trim().to_ascii_lowercase().replace('_', "-");
            v.trim().to_string()
        } else {
            continue;
        };
        match key.as_str() {
            "index-url" => main = words(&value).next().map(String::from),
            "extra-index-url" => extra.extend(words(&value).map(String::from)),
            _ => {}
        }
    }
    (main, extra)
}

struct Config {
    indexes: Vec<Index>,
    /// `[tool.uv.sources]` package -> index name.
    sources: Vec<(String, String)>,
}

fn load(root: &Path, env: &Env) -> Config {
    let mut main: Option<String> = None;
    let mut extras: Vec<Index> = Vec::new();
    let mut named: Vec<Index> = Vec::new();
    let mut sources = Vec::new();

    // pip: config files, then environment.
    let mut pip_files: Vec<PathBuf> = Vec::new();
    pip_files.extend(env.home_file(".pip/pip.conf"));
    pip_files.extend(env.home_file(".config/pip/pip.conf"));
    pip_files.extend(env.get("PIP_CONFIG_FILE").map(PathBuf::from));
    for f in pip_files {
        if let Some(t) = read(&f) {
            let (m, e) = pip_conf(&t);
            main = m.or(main);
            extras.extend(e.into_iter().map(|url| Index {
                name: None,
                url,
                explicit: false,
            }));
        }
    }
    if let Some(v) = env.get("PIP_INDEX_URL") {
        main = Some(v);
    }
    if let Some(v) = env.get("PIP_EXTRA_INDEX_URL") {
        extras.extend(words(&v).map(|url| Index {
            name: None,
            url: url.to_string(),
            explicit: false,
        }));
    }

    // uv: user and project uv.toml, pyproject [tool.uv], environment.
    let mut uv_tables: Vec<toml::Table> = Vec::new();
    for p in [env.home_file(".config/uv/uv.toml"), Some(root.join("uv.toml"))].into_iter().flatten() {
        if let Some(t) = read(&p).and_then(|t| toml::from_str::<toml::Table>(&t).ok()) {
            uv_tables.push(t);
        }
    }
    if let Some(t) = read(&root.join("pyproject.toml"))
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| t.get("tool").and_then(|t| t.get("uv")).and_then(|u| u.as_table().cloned()))
    {
        if let Some(s) = t.get("sources").and_then(|s| s.as_table()) {
            for (pkg, v) in s {
                if let Some(ix) = v.get("index").and_then(|i| i.as_str()) {
                    sources.push((crate::norm_name(Eco::PyPI, pkg), ix.to_string()));
                }
            }
        }
        uv_tables.push(t);
    }
    for t in &uv_tables {
        if let Some(u) = t.get("index-url").and_then(|v| v.as_str()) {
            main = Some(u.to_string());
        }
        if let Some(a) = t.get("extra-index-url").and_then(|v| v.as_array()) {
            extras.extend(a.iter().filter_map(|u| u.as_str()).map(|url| Index {
                name: None,
                url: url.to_string(),
                explicit: false,
            }));
        }
        if let Some(a) = t.get("index").and_then(|v| v.as_array()) {
            for ix in a.iter().filter_map(|i| i.as_table()) {
                let Some(url) = ix.get("url").and_then(|u| u.as_str()) else { continue };
                let i = Index {
                    name: ix.get("name").and_then(|n| n.as_str()).map(String::from),
                    url: url.to_string(),
                    explicit: ix.get("explicit").and_then(|e| e.as_bool()) == Some(true),
                };
                if ix.get("default").and_then(|d| d.as_bool()) == Some(true) {
                    main = Some(url.to_string());
                }
                named.push(i);
            }
        }
    }
    if let Some(v) = env.get("UV_INDEX_URL") {
        main = Some(v);
    }
    if let Some(v) = env.get("UV_EXTRA_INDEX_URL") {
        extras.extend(words(&v).map(|url| Index {
            name: None,
            url: url.to_string(),
            explicit: false,
        }));
    }
    if let Some(v) = env.get("UV_INDEX") {
        for w in words(&v) {
            match w.split_once('=') {
                Some((n, url)) => named.push(Index {
                    name: Some(n.to_string()),
                    url: url.to_string(),
                    explicit: false,
                }),
                None => extras.push(Index {
                    name: None,
                    url: w.to_string(),
                    explicit: false,
                }),
            }
        }
    }

    let mut indexes = Vec::new();
    if let Some(m) = main {
        indexes.push(Index {
            name: None,
            url: m,
            explicit: false,
        });
    }
    indexes.extend(named);
    indexes.extend(extras);
    Config { indexes, sources }
}

/// The registry `uv.lock` recorded for this exact package version.
fn lock_registry(dep: &Dep, root: &Path) -> Option<String> {
    let t: toml::Table = toml::from_str(&read(&root.join("uv.lock"))?).ok()?;
    let want = crate::norm_name(Eco::PyPI, &dep.name);
    t.get("package")?.as_array()?.iter().filter_map(|p| p.as_table()).find_map(|p| {
        let name = crate::norm_name(Eco::PyPI, p.get("name")?.as_str()?);
        (name == want && p.get("version")?.as_str()? == dep.version).then_some(())?;
        p.get("source")?.get("registry")?.as_str().map(String::from)
    })
}

/// Normalize an index URL: no credentials, no trailing slash.
fn clean(url: &str) -> Option<String> {
    Url::parse(url).map(|u| u.clean().trim_end_matches('/').to_string())
}

/// The credential for an index URL: userinfo, then named env variables, then netrc.
fn credential(ix: &Index, env: &Env) -> Option<Credential> {
    let u = Url::parse(&ix.url)?;
    if let Some((user, pass)) = &u.userinfo {
        let auth = match pass {
            Some(p) => Auth::Basic(user.clone(), p.clone()),
            // A lone token in the user field (`https://TOKEN@host/`): the usual bearer shorthand.
            None => Auth::Basic(user.clone(), String::new()),
        };
        return Some(Credential::new(u.origin(), auth));
    }
    if let Some(n) = &ix.name {
        let key = n.to_ascii_uppercase().replace(['-', '.'], "_");
        if let Some(pw) = env.get(&format!("UV_INDEX_{key}_PASSWORD")) {
            let user = env.get(&format!("UV_INDEX_{key}_USERNAME")).unwrap_or_default();
            return Some(Credential::new(u.origin(), Auth::Basic(user, pw)));
        }
    }
    netrc_for(env, &u)
}

pub fn plan(dep: &Dep, root: &Path, env: &Env) -> Plan {
    let cfg = load(root, env);
    let mut creds = Creds::default();
    let mut candidates = Vec::new();
    let add = |url: &str, definitive: bool, candidates: &mut Vec<Candidate>, creds: &mut Creds| {
        let Some(base) = clean(url) else { return };
        if is_public(&base) || candidates.iter().any(|c| c.base == base) {
            return;
        }
        if let Some(c) = credential(
            &Index {
                name: None,
                url: url.to_string(),
                explicit: false,
            },
            env,
        ) {
            creds.push(c);
        }
        candidates.push(Candidate {
            base,
            private: true,
            definitive,
            pinned: None,
        });
    };
    let by_url = |url: &str| cfg.indexes.iter().find(|i| clean(&i.url) == clean(url));
    let register = |ix: &Index, creds: &mut Creds| {
        if let Some(c) = credential(ix, env) {
            creds.push(c);
        }
    };
    // 1. the lockfile's recorded registry; 2. `tool.uv.sources`.
    let mut pinned: Option<String> = lock_registry(dep, root);
    if pinned.is_none() {
        let want = crate::norm_name(Eco::PyPI, &dep.name);
        pinned = cfg
            .sources
            .iter()
            .find(|(p, _)| *p == want)
            .and_then(|(_, n)| cfg.indexes.iter().find(|i| i.name.as_deref() == Some(n)))
            .map(|i| i.url.clone());
    }
    if let Some(p) = pinned {
        if !is_public(&p) {
            if let Some(ix) = by_url(&p) {
                register(ix, &mut creds);
            }
            add(&p, true, &mut candidates, &mut creds);
            if !candidates.is_empty() {
                return Plan { candidates, creds };
            }
        } else {
            // The lockfile recorded a public registry: honour it. A same-name
            // and same-version artifact on a configured private index must not
            // win over the artifact `uv.lock` actually pinned.
            if let Some(ix) = by_url(&p) {
                register(ix, &mut creds);
            }
            candidates.push(Candidate {
                base: clean(&p).unwrap_or_else(|| DEFAULT.into()),
                private: false,
                definitive: true,
                pinned: None,
            });
            return Plan { candidates, creds };
        }
    }
    for ix in cfg.indexes.iter().filter(|i| !i.explicit) {
        if !is_public(&ix.url) {
            register(ix, &mut creds);
        }
        add(&ix.url, false, &mut candidates, &mut creds);
    }
    candidates.push(Candidate {
        base: DEFAULT.into(),
        private: false,
        definitive: false,
        pinned: None,
    });
    Plan { candidates, creds }
}

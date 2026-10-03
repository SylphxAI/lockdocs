//! Python private indexes, from uv (`uv.toml`, `[[tool.uv.index]]`,
//! `UV_INDEX_*`), pip (`PIP_INDEX_URL`, `PIP_EXTRA_INDEX_URL`, `pip.conf`),
//! credentials in the URL, `UV_INDEX_<NAME>_USERNAME/_PASSWORD` or `~/.netrc`,
//! and the index `uv.lock` recorded per package.

use super::*;

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
fn pip_conf(text: &str) -> (Option<String>, Vec<String>) {
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
    if let Some(p) = pinned.filter(|p| !is_public(p)) {
        if let Some(ix) = by_url(&p) {
            register(ix, &mut creds);
        }
        add(&p, true, &mut candidates, &mut creds);
        if !candidates.is_empty() {
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

fn file_version(name: &str) -> Option<(String, String)> {
    let lower = name.to_string();
    if let Some(stem) = lower.strip_suffix(".whl") {
        let mut it = stem.split('-');
        return Some((it.next()?.to_string(), it.next()?.to_string()));
    }
    for ext in [".tar.gz", ".zip"] {
        if let Some(stem) = lower.strip_suffix(ext) {
            let (n, v) = stem.rsplit_once('-')?;
            return Some((n.to_string(), v.to_string()));
        }
    }
    None
}

/// `(filename, url)` pairs of a simple-index page (PEP 691 JSON or PEP 503 HTML).
pub fn parse_simple(body: &[u8], page_url: &str) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(body);
    let base = Url::parse(page_url);
    let resolve = |href: &str| -> Option<String> {
        let href = href.split('#').next().unwrap_or(href).replace("&amp;", "&");
        match &base {
            Some(b) => b.join(&href).map(|u| u.clean()),
            None => None,
        }
    };
    if text.trim_start().starts_with('{') {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Vec::new();
        };
        return v
            .get("files")
            .and_then(|f| f.as_array())
            .into_iter()
            .flatten()
            .filter_map(|f| Some((f.get("filename")?.as_str()?.to_string(), resolve(f.get("url")?.as_str()?)?)))
            .collect();
    }
    let mut out = Vec::new();
    let mut rest = &*text;
    while let Some(i) = rest.find("<a ") {
        rest = &rest[i..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        let after = &rest[end + 1..];
        let href = tag.find("href=").and_then(|h| {
            let v = &tag[h + 5..];
            let q = v.chars().next()?;
            (q == '"' || q == '\'').then(|| v[1..].split(q).next().unwrap_or("").to_string())
        });
        let label = after.split("</a>").next().unwrap_or("").trim().to_string();
        if let (Some(h), false) = (href, label.is_empty()) {
            if let Some(u) = resolve(&h) {
                out.push((label, u));
            }
        }
        rest = after;
    }
    out
}

/// Pick the artifact for `version`: a pure wheel, any wheel, then an sdist.
pub fn pick(files: &[(String, String)], name: &str, version: &str) -> Option<String> {
    let want = crate::norm_name(Eco::PyPI, name);
    let matching: Vec<&(String, String)> = files
        .iter()
        .filter(|(f, _)| file_version(f).is_some_and(|(n, v)| crate::norm_name(Eco::PyPI, &n) == want && v == version))
        .collect();
    matching
        .iter()
        .find(|(f, _)| f.ends_with("-none-any.whl"))
        .or_else(|| matching.iter().find(|(f, _)| f.ends_with(".whl")))
        .or_else(|| matching.iter().find(|(f, _)| f.ends_with(".tar.gz") || f.ends_with(".zip")))
        .map(|(_, u)| u.clone())
}

/// Download one version from a private index. Ok(false) when it has none.
pub fn download(http: &Http, dep: &Dep, c: &Candidate, dir: &Path) -> Result<bool> {
    let page = format!("{}/{}/", c.base.trim_end_matches('/'), crate::norm_name(Eco::PyPI, &dep.name));
    let r = http.get(&page, Some("application/vnd.pypi.simple.v1+json, text/html;q=0.5"))?;
    match r.status {
        200 => {}
        404 | 410 => return Ok(false),
        401 | 403 => bail!(
            "{} refused the request (HTTP {}); check the index credential",
            Url::parse(&page).map_or_else(String::new, |u| u.origin().to_string()),
            r.status
        ),
        s => bail!("{}: HTTP {s}", r.url),
    }
    let files = parse_simple(&r.body, &r.url);
    let Some(url) = pick(&files, &dep.name, &dep.version) else { return Ok(false) };
    let Some(bytes) = http.get_ok(&url, None)? else { return Ok(false) };
    if url.ends_with(".whl") || url.ends_with(".zip") {
        crate::fetch::unzip(&bytes, dir, url.ends_with(".zip"), None)?;
    } else {
        crate::fetch::untar_gz(&bytes, dir)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    fn dep(name: &str, v: &str) -> Dep {
        Dep {
            eco: Eco::PyPI,
            name: name.into(),
            version: v.into(),
            direct: true,
            from: "uv.lock".into(),
        }
    }

    #[test]
    fn pip_conf_and_env_indexes() {
        let (m, e) = pip_conf("[global]\nindex-url = https://pypi.org/simple\nextra-index-url =\n  https://a.example/simple\n  https://b.example/simple\n[other]\nindex-url=https://no.example\n");
        assert_eq!(m.as_deref(), Some("https://pypi.org/simple"));
        assert_eq!(e, ["https://a.example/simple", "https://b.example/simple"]);
        let t = tempfile::tempdir().unwrap();
        let env = Env::fixed(&[("PIP_EXTRA_INDEX_URL", "https://__token__:SECRET-PIP@priv.example/simple")], None);
        let p = plan(&dep("acme-lib", "1.0.0"), t.path(), &env);
        assert!(p.any_private() && !p.definitely_private());
        assert_eq!(p.candidates[0].base, "https://priv.example/simple");
        assert!(!format!("{p:?}").contains("SECRET-PIP"));
        let auth = p.creds.for_url(&Url::parse("https://priv.example/simple/acme-lib/").unwrap()).unwrap();
        assert_eq!(auth.header(), Auth::Basic("__token__".into(), "SECRET-PIP".into()).header());
        assert!(p.creds.for_url(&Url::parse("https://pypi.org/simple/x/").unwrap()).is_none());
    }

    #[test]
    fn uv_index_named_credentials_sources_and_lock() {
        let t = tempfile::tempdir().unwrap();
        write(
            &t.path().join("pyproject.toml"),
            "[tool.uv.sources]\nacme-lib = { index = \"acme\" }\n[[tool.uv.index]]\nname = \"acme\"\nurl = \"https://py.acme.example/simple\"\nexplicit = true\n",
        );
        let env = Env::fixed(&[("UV_INDEX_ACME_USERNAME", "ci"), ("UV_INDEX_ACME_PASSWORD", "SECRET-UV")], None);
        let p = plan(&dep("acme_lib", "2.0.0"), t.path(), &env);
        assert!(p.definitely_private());
        assert_eq!(
            p.creds.for_url(&Url::parse("https://py.acme.example/simple/x/").unwrap()).unwrap().header(),
            Auth::Basic("ci".into(), "SECRET-UV".into()).header()
        );
        // An explicit index serves only the packages pinned to it.
        assert!(!plan(&dep("requests", "2.0.0"), t.path(), &env).any_private());

        let t2 = tempfile::tempdir().unwrap();
        write(
            &t2.path().join("uv.lock"),
            "version = 1\n[[package]]\nname = \"acme-lib\"\nversion = \"2.0.0\"\nsource = { registry = \"https://lock.example/simple\" }\n[[package]]\nname = \"requests\"\nversion = \"2.0.0\"\nsource = { registry = \"https://pypi.org/simple\" }\n",
        );
        let p = plan(&dep("acme-lib", "2.0.0"), t2.path(), &Env::fixed(&[], None));
        assert!(p.definitely_private() && p.candidates[0].base == "https://lock.example/simple");
        assert!(!plan(&dep("requests", "2.0.0"), t2.path(), &Env::fixed(&[], None)).any_private());
    }

    #[test]
    fn netrc_credential_for_the_index_host() {
        let home = tempfile::tempdir().unwrap();
        write(&home.path().join(".netrc"), "machine priv.example login me password SECRET-NETRC\n");
        let t = tempfile::tempdir().unwrap();
        let env = Env::fixed(&[("PIP_INDEX_URL", "https://priv.example/simple")], Some(home.path()));
        let p = plan(&dep("acme-lib", "1.0.0"), t.path(), &env);
        assert!(p.creds.for_url(&Url::parse("https://priv.example/simple/a/").unwrap()).is_some());
        assert!(p.creds.for_url(&Url::parse("https://pypi.org/simple/a/").unwrap()).is_none());
    }

    #[test]
    fn simple_pages_html_and_json() {
        let html = br#"<a href="../../files/acme_lib-1.0.0-py3-none-any.whl#sha256=ab">acme_lib-1.0.0-py3-none-any.whl</a><a href="acme-lib-1.0.0.tar.gz">acme-lib-1.0.0.tar.gz</a>"#;
        let f = parse_simple(html, "https://p.example/simple/acme-lib/");
        assert_eq!(
            pick(&f, "acme-lib", "1.0.0").unwrap(),
            "https://p.example/files/acme_lib-1.0.0-py3-none-any.whl"
        );
        assert!(pick(&f, "acme-lib", "2.0.0").is_none());
        let json = br#"{"files":[{"filename":"acme-lib-1.0.0.tar.gz","url":"https://f.example/a.tar.gz"}]}"#;
        assert_eq!(
            pick(&parse_simple(json, "https://p.example/simple/acme-lib/"), "acme-lib", "1.0.0").unwrap(),
            "https://f.example/a.tar.gz"
        );
    }

    #[test]
    fn downloads_from_a_private_index_with_basic_auth() {
        let tar = tgz(&[("acme-lib-1.0.0/README.md", b"# acme"), ("acme-lib-1.0.0/acme.py", b"x = 1")]);
        let m = serve(move |p, auth| {
            if auth != Some("Basic dXNlcjpTRUNSRVQtSURY") {
                return (401, "text/plain".into(), vec![], vec![]);
            }
            match p {
                "/simple/acme-lib/" => (
                    200,
                    "text/html".into(),
                    br#"<a href="/files/acme-lib-1.0.0.tar.gz#sha256=00">acme-lib-1.0.0.tar.gz</a>"#.to_vec(),
                    vec![],
                ),
                "/files/acme-lib-1.0.0.tar.gz" => (200, "application/gzip".into(), tar.clone(), vec![]),
                _ => (404, "text/plain".into(), vec![], vec![]),
            }
        });
        let t = tempfile::tempdir().unwrap();
        let url = format!("http://user:SECRET-IDX@{}/simple", m.host);
        let env = Env::fixed(&[("PIP_EXTRA_INDEX_URL", &url)], None);
        let p = plan(&dep("acme-lib", "1.0.0"), t.path(), &env);
        let out = tempfile::tempdir().unwrap();
        assert!(download(&Http::new(p.creds), &dep("acme-lib", "1.0.0"), &p.candidates[0], out.path()).unwrap());
        assert_eq!(std::fs::read_to_string(out.path().join("README.md")).unwrap(), "# acme");
    }
}

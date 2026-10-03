//! Cargo alternate registries (sparse protocol), from `.cargo/config.toml`,
//! `$CARGO_HOME/credentials.toml`, `CARGO_REGISTRIES_<NAME>_*` variables, the
//! registry `Cargo.lock` recorded per package and the `registry = "name"` key
//! of `Cargo.toml` dependencies.

use super::*;

const PUBLIC_HOSTS: &[&str] = &["crates.io", "index.crates.io", "static.crates.io", "github.com"];

fn cargo_home(env: &Env) -> Option<PathBuf> {
    env.get("CARGO_HOME").map(PathBuf::from).or_else(|| env.home_file(".cargo"))
}

fn table_of(p: &Path) -> Option<toml::Table> {
    toml::from_str(&read(p)?).ok()
}

/// Config tables, lowest priority first: `$CARGO_HOME`, then each ancestor
/// directory's `.cargo/`, the project's last.
fn configs(root: &Path, env: &Env) -> Vec<toml::Table> {
    let mut dirs: Vec<PathBuf> = cargo_home(env).into_iter().collect();
    let mut up: Vec<PathBuf> = root.ancestors().map(|a| a.join(".cargo")).collect();
    up.reverse();
    dirs.extend(up);
    let mut out = Vec::new();
    for d in dirs {
        for n in ["config.toml", "config"] {
            if let Some(t) = table_of(&d.join(n)) {
                out.push(t);
                break;
            }
        }
    }
    out
}

fn norm_index(s: &str) -> String {
    s.trim()
        .strip_prefix("sparse+")
        .or_else(|| s.trim().strip_prefix("registry+"))
        .unwrap_or(s.trim())
        .trim_end_matches('/')
        .to_string()
}

fn is_public(index: &str) -> bool {
    let i = norm_index(index);
    Url::parse(&i).is_none_or(|u| PUBLIC_HOSTS.contains(&u.host.as_str()) && (u.host != "github.com" || u.path.contains("crates.io-index")))
}

struct Registry {
    name: String,
    index: String,
}

fn registries(cfgs: &[toml::Table], env: &Env) -> Vec<Registry> {
    let mut out: Vec<Registry> = Vec::new();
    let mut put = |name: &str, index: &str| {
        out.retain(|r| r.name != name);
        out.push(Registry {
            name: name.to_string(),
            index: index.to_string(),
        });
    };
    for t in cfgs {
        if let Some(r) = t.get("registries").and_then(|r| r.as_table()) {
            for (n, v) in r {
                if let Some(i) = v.get("index").and_then(|i| i.as_str()) {
                    put(n, i);
                }
            }
        }
        if let Some(s) = t.get("source").and_then(|s| s.as_table()) {
            for (n, v) in s {
                if let Some(i) = v.get("registry").and_then(|i| i.as_str()) {
                    put(n, i);
                }
            }
        }
    }
    for (k, v) in env.with_prefix("CARGO_REGISTRIES_") {
        if let Some(n) = k.to_ascii_uppercase().strip_prefix("CARGO_REGISTRIES_").and_then(|r| r.strip_suffix("_INDEX")) {
            put(&n.to_ascii_lowercase().replace('_', "-"), &v);
        }
    }
    out
}

/// The token for a registry name: environment, then credentials file, then config.
fn token(name: &str, cfgs: &[toml::Table], creds_files: &[toml::Table], env: &Env) -> Option<String> {
    let key = format!("CARGO_REGISTRIES_{}_TOKEN", name.to_ascii_uppercase().replace('-', "_"));
    if let Some(t) = env.get(&key) {
        return Some(t);
    }
    creds_files
        .iter()
        .rev()
        .chain(cfgs.iter().rev())
        .find_map(|t| t.get("registries")?.get(name)?.get("token")?.as_str().map(String::from))
}

fn lock_entry(dep: &Dep, root: &Path) -> Option<String> {
    let t = table_of(&root.join("Cargo.lock"))?;
    let want = crate::norm_name(Eco::Cargo, &dep.name);
    t.get("package")?.as_array()?.iter().filter_map(|p| p.as_table()).find_map(|p| {
        (crate::norm_name(Eco::Cargo, p.get("name")?.as_str()?) == want && p.get("version")?.as_str()? == dep.version).then_some(())?;
        p.get("source")?.as_str().map(String::from)
    })
}

/// The registry name a `Cargo.toml` dependency asks for.
fn manifest_registry(dep: &Dep, root: &Path) -> Option<String> {
    let t = table_of(&root.join("Cargo.toml"))?;
    let want = crate::norm_name(Eco::Cargo, &dep.name);
    let mut tables: Vec<&toml::Table> = Vec::new();
    for k in ["dependencies", "dev-dependencies", "build-dependencies"] {
        tables.extend(t.get(k).and_then(|d| d.as_table()));
    }
    tables.extend(t.get("workspace").and_then(|w| w.get("dependencies")).and_then(|d| d.as_table()));
    tables.into_iter().find_map(|d| {
        d.iter().find_map(|(k, v)| {
            let real = v.get("package").and_then(|p| p.as_str()).unwrap_or(k);
            (crate::norm_name(Eco::Cargo, real) == want).then(|| v.get("registry")?.as_str().map(String::from))?
        })
    })
}

pub fn plan(dep: &Dep, root: &Path, env: &Env) -> Plan {
    let cfgs = configs(root, env);
    let creds_files: Vec<toml::Table> = cargo_home(env)
        .into_iter()
        .flat_map(|h| ["credentials.toml", "credentials"].map(|n| h.join(n)))
        .filter_map(|p| table_of(&p))
        .collect();
    let regs = registries(&cfgs, env);
    let mut creds = Creds::default();
    let mut candidates = Vec::new();
    let add = |index: &str, name: Option<&str>, definitive: bool, git: bool, candidates: &mut Vec<Candidate>, creds: &mut Creds| {
        let base = norm_index(index);
        let Some(u) = Url::parse(&base) else { return };
        if candidates.iter().any(|c: &Candidate| c.base == base) {
            return;
        }
        let name = name
            .map(String::from)
            .or_else(|| regs.iter().find(|r| norm_index(&r.index) == base).map(|r| r.name.clone()));
        if let Some(t) = name.and_then(|n| token(&n, &cfgs, &creds_files, env)) {
            creds.push(Credential::new(u.origin(), Auth::Raw(t)));
        }
        candidates.push(Candidate {
            base,
            private: true,
            definitive,
            pinned: git.then(|| "git-index".to_string()),
        });
    };
    // 1. Cargo.lock; 2. the manifest's `registry = "name"`.
    if let Some(src) = lock_entry(dep, root) {
        if !is_public(&src) {
            add(
                &src,
                None,
                true,
                src.starts_with("registry+") && !src.contains("sparse+"),
                &mut candidates,
                &mut creds,
            );
        }
    }
    if candidates.is_empty() {
        if let Some(r) = manifest_registry(dep, root).and_then(|n| regs.iter().find(|r| r.name == n)) {
            add(&r.index, Some(&r.name), true, !r.index.starts_with("sparse+"), &mut candidates, &mut creds);
        }
    }
    // 3. crates.io replaced by a mirror.
    if candidates.is_empty() {
        let replaced = cfgs
            .iter()
            .rev()
            .find_map(|t| t.get("source")?.get("crates-io")?.get("replace-with")?.as_str().map(String::from));
        if let Some(r) = replaced.and_then(|n| regs.iter().find(|r| r.name == n)) {
            add(&r.index, Some(&r.name), false, !r.index.starts_with("sparse+"), &mut candidates, &mut creds);
        }
    }
    if candidates.first().is_none_or(|c| !c.definitive) {
        candidates.push(Candidate {
            base: "https://index.crates.io".into(),
            private: false,
            definitive: false,
            pinned: None,
        });
    }
    Plan { candidates, creds }
}

fn prefix(name: &str) -> String {
    match name.len() {
        1 => "1".into(),
        2 => "2".into(),
        3 => format!("3/{}", &name[..1]),
        _ => format!("{}/{}", &name[..2], &name[2..4]),
    }
}

/// The download URL from the registry's `dl` template.
pub fn dl_url(dl: &str, krate: &str, version: &str) -> Option<String> {
    if !dl.contains(['{']) {
        return Some(format!("{}/{krate}/{version}/download", dl.trim_end_matches('/')));
    }
    let out = dl
        .replace("{crate}", krate)
        .replace("{version}", version)
        .replace("{prefix}", &prefix(krate))
        .replace("{lowerprefix}", &prefix(&krate.to_ascii_lowercase()));
    (!out.contains("{sha256-checksum}")).then_some(out)
}

/// Download one version from a sparse registry. Ok(false) when it has none.
pub fn download(http: &Http, dep: &Dep, c: &Candidate, dir: &Path) -> Result<bool> {
    if c.pinned.as_deref() == Some("git-index") {
        bail!("this registry uses the git index protocol; lockdocs reads sparse registries (`sparse+https://...`)");
    }
    let base = c.base.trim_end_matches('/');
    let Some(cfg) = http.get_ok(&format!("{base}/config.json"), None)? else {
        return Ok(false);
    };
    let cfg: serde_json::Value = serde_json::from_slice(&cfg)?;
    let Some(dl) = cfg.get("dl").and_then(|d| d.as_str()) else {
        bail!("the registry's config.json has no `dl` template");
    };
    let Some(url) = dl_url(dl, &dep.name, &dep.version) else {
        bail!("the registry's `dl` template uses {{sha256-checksum}}, which lockdocs does not read yet");
    };
    let url = if url.starts_with('/') {
        Url::parse(base).and_then(|b| b.join(&url)).map_or(url.clone(), |u| u.clean())
    } else {
        url
    };
    let Some(bytes) = http.get_ok(&url, None)? else { return Ok(false) };
    crate::fetch::untar_gz(&bytes, dir)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    fn dep(name: &str, v: &str) -> Dep {
        Dep {
            eco: Eco::Cargo,
            name: name.into(),
            version: v.into(),
            direct: true,
            from: "Cargo.lock".into(),
        }
    }

    #[test]
    fn lock_source_config_and_token() {
        let t = tempfile::tempdir().unwrap();
        write(&t.path().join("Cargo.lock"), "version = 4\n[[package]]\nname = \"acme-core\"\nversion = \"0.3.0\"\nsource = \"sparse+https://cargo.acme.example/index/\"\n[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n");
        write(
            &t.path().join(".cargo/config.toml"),
            "[registries.acme]\nindex = \"sparse+https://cargo.acme.example/index/\"\n",
        );
        let home = tempfile::tempdir().unwrap();
        write(&home.path().join(".cargo/credentials.toml"), "[registries.acme]\ntoken = \"SECRET-CRED\"\n");
        let env = Env::fixed(&[], Some(home.path()));
        let p = plan(&dep("acme-core", "0.3.0"), t.path(), &env);
        assert!(p.definitely_private());
        assert_eq!(p.candidates[0].base, "https://cargo.acme.example/index");
        let auth = p.creds.for_url(&Url::parse("https://cargo.acme.example/index/config.json").unwrap()).unwrap();
        assert_eq!(auth.header(), "SECRET-CRED");
        assert!(p.creds.for_url(&Url::parse("https://index.crates.io/config.json").unwrap()).is_none());
        assert!(!plan(&dep("serde", "1.0.0"), t.path(), &env).any_private());
        // The environment token wins over the credentials file.
        let env2 = Env::fixed(&[("CARGO_REGISTRIES_ACME_TOKEN", "SECRET-ENV")], Some(home.path()));
        let p2 = plan(&dep("acme-core", "0.3.0"), t.path(), &env2);
        assert_eq!(
            p2.creds.for_url(&Url::parse("https://cargo.acme.example/index/x").unwrap()).unwrap().header(),
            "SECRET-ENV"
        );
    }

    #[test]
    fn manifest_registry_key_without_lock() {
        let t = tempfile::tempdir().unwrap();
        write(
            &t.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\n[dependencies]\nacme-core = { version = \"0.3\", registry = \"acme\" }\n",
        );
        write(
            &t.path().join(".cargo/config.toml"),
            "[registries.acme]\nindex = \"sparse+https://cargo.acme.example/index/\"\n",
        );
        let p = plan(&dep("acme-core", "0.3.0"), t.path(), &Env::fixed(&[], None));
        assert!(p.definitely_private());
    }

    #[test]
    fn git_index_registry_is_refused_clearly() {
        let t = tempfile::tempdir().unwrap();
        write(
            &t.path().join("Cargo.lock"),
            "version = 4\n[[package]]\nname = \"a\"\nversion = \"1.0.0\"\nsource = \"registry+https://git.acme.example/index\"\n",
        );
        let p = plan(&dep("a", "1.0.0"), t.path(), &Env::fixed(&[], None));
        let e = download(&Http::new(Creds::default()), &dep("a", "1.0.0"), &p.candidates[0], t.path())
            .unwrap_err()
            .to_string();
        assert!(e.contains("sparse"), "{e}");
    }

    #[test]
    fn dl_templates() {
        assert_eq!(
            dl_url("https://d.example/api", "acme", "1.0.0").unwrap(),
            "https://d.example/api/acme/1.0.0/download"
        );
        assert_eq!(
            dl_url("https://d.example/{prefix}/{crate}-{version}.crate", "acme", "1.0.0").unwrap(),
            "https://d.example/ac/me/acme-1.0.0.crate"
        );
        assert_eq!(
            dl_url("https://d.example/{lowerprefix}/{crate}", "ab", "1.0.0").unwrap(),
            "https://d.example/2/ab"
        );
        assert!(dl_url("https://d.example/{sha256-checksum}", "ab", "1").is_none());
    }

    #[test]
    fn downloads_with_the_raw_token() {
        let krate = tgz(&[("acme-core-0.3.0/README.md", b"# acme"), ("acme-core-0.3.0/Cargo.toml", b"[package]")]);
        let m = serve(move |p, auth| {
            if auth != Some("SECRET-TOK") {
                return (401, "text/plain".into(), vec![], vec![]);
            }
            match p {
                "/index/config.json" => (200, "application/json".into(), br#"{"dl":"/dl/{crate}/{version}.crate"}"#.to_vec(), vec![]),
                "/dl/acme-core/0.3.0.crate" => (200, "application/gzip".into(), krate.clone(), vec![]),
                _ => (404, "text/plain".into(), vec![], vec![]),
            }
        });
        let t = tempfile::tempdir().unwrap();
        write(
            &t.path().join("Cargo.lock"),
            &format!(
                "version = 4\n[[package]]\nname = \"acme-core\"\nversion = \"0.3.0\"\nsource = \"sparse+{}/index/\"\n",
                m.base
            ),
        );
        let env = Env::fixed(&[("CARGO_REGISTRIES_TEST_TOKEN", "SECRET-TOK")], None);
        write(
            &t.path().join(".cargo/config.toml"),
            &format!("[registries.test]\nindex = \"sparse+{}/index/\"\n", m.base),
        );
        let p = plan(&dep("acme-core", "0.3.0"), t.path(), &env);
        let out = tempfile::tempdir().unwrap();
        assert!(download(&Http::new(p.creds), &dep("acme-core", "0.3.0"), &p.candidates[0], out.path()).unwrap());
        assert!(out.path().join("README.md").is_file());
    }
}

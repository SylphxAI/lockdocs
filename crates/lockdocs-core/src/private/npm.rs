//! npm private registries, from the user's own `.npmrc` files and
//! `NPM_CONFIG_*` variables, and from `package-lock.json`'s recorded URLs.

use super::*;
use std::collections::BTreeMap;

const PUBLIC_HOSTS: &[&str] = &["registry.npmjs.org", "registry.yarnpkg.com"];
const DEFAULT: &str = "https://registry.npmjs.org";

fn is_public(url: &str) -> bool {
    Url::parse(url).is_none_or(|u| PUBLIC_HOSTS.contains(&u.host.as_str()))
}

/// `${VAR}` expansion; `None` when a variable is unset (that entry is skipped).
fn expand(v: &str, env: &Env) -> Option<String> {
    let mut out = String::new();
    let mut rest = v;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let end = rest[i..].find('}')? + i;
        let name = rest[i + 2..end].trim_end_matches('?');
        out.push_str(&env.get(name)?);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// The `key=value` pairs of one `.npmrc`.
pub fn parse_npmrc(text: &str, env: &Env) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with(';') || l.starts_with('#') {
            continue;
        }
        let Some((k, v)) = l.split_once('=') else { continue };
        let v = v.trim().trim_matches(['"', '\'']);
        if let Some(v) = expand(v, env) {
            out.push((k.trim().to_string(), v));
        }
    }
    out
}

/// Merged configuration: user file, then project file, then environment.
fn config(root: &Path, env: &Env) -> BTreeMap<String, String> {
    let mut cfg = BTreeMap::new();
    let user = env.get("NPM_CONFIG_USERCONFIG").map(PathBuf::from).or_else(|| env.home_file(".npmrc"));
    for p in user.into_iter().chain([root.join(".npmrc")]) {
        if let Some(t) = read(&p) {
            cfg.extend(parse_npmrc(&t, env));
        }
    }
    for (k, v) in env.with_prefix("npm_config_") {
        let key = &k["npm_config_".len()..];
        if key.eq_ignore_ascii_case("userconfig") {
            continue;
        }
        let key = if key.starts_with("//") || key.starts_with('@') {
            key.to_string()
        } else {
            key.to_ascii_lowercase().replace('_', "-")
        };
        cfg.insert(key, v);
    }
    cfg
}

fn scope(name: &str) -> Option<&str> {
    name.starts_with('@').then(|| name.split('/').next()).flatten()
}

/// Credentials for every `//host[:port]/path/:key` entry, each bound to the
/// scheme of a configured registry on that host (https otherwise).
fn credentials(cfg: &BTreeMap<String, String>, registries: &[String]) -> Creds {
    let hostport_of = |u: &Url| {
        if u.port == if u.scheme == "https" { 443 } else { 80 } {
            u.host.clone()
        } else {
            format!("{}:{}", u.host, u.port)
        }
    };
    let scheme_for = |hostport: &str| -> (String, u16) {
        for r in registries {
            if let Some(u) = Url::parse(r) {
                if hostport_of(&u) == hostport {
                    return (u.scheme, u.port);
                }
            }
        }
        match hostport.rsplit_once(':') {
            Some((_, p)) if p.chars().all(|c| c.is_ascii_digit()) => ("https".to_string(), p.parse().unwrap_or(443)),
            _ => ("https".to_string(), 443),
        }
    };
    let mut creds = Creds::default();
    let mut by_prefix: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (k, v) in cfg {
        let Some(rest) = k.strip_prefix("//") else { continue };
        if let Some((nerf, field)) = rest.rsplit_once(":_") {
            by_prefix.entry(nerf.to_string()).or_default().insert(format!("_{field}"), v.clone());
        } else if let Some((nerf, _)) = rest.rsplit_once(":username") {
            by_prefix.entry(nerf.to_string()).or_default().insert("username".into(), v.clone());
        }
    }
    for (nerf, f) in by_prefix {
        let (hostport, path) = match nerf.find('/') {
            Some(i) => (nerf[..i].to_ascii_lowercase(), nerf[i..].to_string()),
            None => (nerf.to_ascii_lowercase(), "/".to_string()),
        };
        let auth = if let Some(t) = f.get("_authToken") {
            Auth::Bearer(t.clone())
        } else if let Some(a) = f.get("_auth") {
            match b64_decode(a)
                .and_then(|b| String::from_utf8(b).ok())
                .and_then(|s| s.split_once(':').map(|(u, p)| (u.to_string(), p.to_string())))
            {
                Some((u, p)) => Auth::Basic(u, p),
                None => continue,
            }
        } else if let (Some(u), Some(p)) = (f.get("username"), f.get("_password")) {
            let Some(p) = b64_decode(p).and_then(|b| String::from_utf8(b).ok()) else {
                continue;
            };
            Auth::Basic(u.clone(), p)
        } else {
            continue;
        };
        let (scheme, port) = scheme_for(&hostport);
        let host = hostport
            .rsplit_once(':')
            .filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit()))
            .map_or(hostport.clone(), |(h, _)| h.to_string());
        creds.push(Credential {
            origin: Origin { scheme, host, port },
            path_prefix: (path != "/").then_some(path),
            auth,
        });
    }
    creds
}

/// The tarball URL `package-lock.json` recorded for this exact version.
fn lock_resolved(dep: &Dep, root: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&read(&root.join("package-lock.json"))?).ok()?;
    let pk = v.get("packages")?.as_object()?;
    let suffix = format!("node_modules/{}", dep.name);
    pk.iter()
        .filter(|(k, e)| {
            (k.as_str() == suffix || k.ends_with(&format!("/{suffix}"))) && e.get("version").and_then(|x| x.as_str()) == Some(dep.version.as_str())
        })
        .find_map(|(_, e)| e.get("resolved").and_then(|r| r.as_str()).filter(|r| r.starts_with("http")).map(String::from))
}

pub fn plan(dep: &Dep, root: &Path, env: &Env) -> Plan {
    let cfg = config(root, env);
    let reg = scope(&dep.name)
        .and_then(|s| cfg.get(&format!("{s}:registry")))
        .or_else(|| cfg.get("registry"))
        .cloned()
        .unwrap_or_else(|| DEFAULT.to_string())
        .trim_end_matches('/')
        .to_string();
    let mut registries: Vec<String> = cfg
        .iter()
        .filter(|(k, _)| k.as_str() == "registry" || k.ends_with(":registry"))
        .map(|(_, v)| v.clone())
        .collect();
    registries.push(reg.clone());
    let pinned = lock_resolved(dep, root);
    if let Some(p) = &pinned {
        registries.push(p.clone());
    }
    let creds = credentials(&cfg, &registries);
    let mut candidates = Vec::new();
    if let Some(p) = pinned.filter(|p| !is_public(p)) {
        let base = Url::parse(&p).map(|u| u.origin().to_string()).unwrap_or_default();
        candidates.push(Candidate {
            base,
            private: true,
            definitive: true,
            pinned: Some(p),
        });
    } else if !is_public(&reg) {
        candidates.push(Candidate {
            base: reg,
            private: true,
            // A scoped registry is the user saying "this scope lives there".
            definitive: scope(&dep.name).is_some_and(|s| cfg.contains_key(&format!("{s}:registry"))),
            pinned: None,
        });
    }
    if candidates.first().is_none_or(|c| !c.definitive) {
        candidates.push(Candidate {
            base: DEFAULT.into(),
            private: false,
            definitive: false,
            pinned: None,
        });
    }
    Plan { candidates, creds }
}

/// Download one version from a private registry. Ok(false) when it has none.
pub fn download(http: &Http, dep: &Dep, c: &Candidate, dir: &Path) -> Result<bool> {
    let tarball = match &c.pinned {
        Some(p) => p.clone(),
        None => {
            let url = format!("{}/{}/{}", c.base.trim_end_matches('/'), dep.name.replace('/', "%2F"), dep.version);
            let Some(body) = http.get_ok(&url, Some("application/json"))? else {
                return Ok(false);
            };
            let meta: serde_json::Value = serde_json::from_slice(&body)?;
            match meta.pointer("/dist/tarball").and_then(|t| t.as_str()) {
                Some(t) => t.to_string(),
                None => bail!("{}: the registry returned no tarball for this version", dep.id()),
            }
        }
    };
    let Some(bytes) = http.get_ok(&tarball, None)? else { return Ok(false) };
    crate::fetch::untar_gz(&bytes, dir)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    fn dep(name: &str, v: &str) -> Dep {
        Dep {
            eco: Eco::Npm,
            name: name.into(),
            version: v.into(),
            direct: true,
            from: "package-lock.json".into(),
        }
    }

    #[test]
    fn scoped_registry_and_token_from_project_npmrc() {
        let t = tempfile::tempdir().unwrap();
        write(
            &t.path().join(".npmrc"),
            "@acme:registry=https://npm.acme.example/repo/\n//npm.acme.example/repo/:_authToken=${ACME_TOKEN}\n//other.example/:_authToken=OTHER-SECRET\n",
        );
        let env = Env::fixed(&[("ACME_TOKEN", "SECRET-ACME")], None);
        let p = plan(&dep("@acme/ui", "1.0.0"), t.path(), &env);
        assert!(p.definitely_private());
        assert_eq!(p.candidates[0].base, "https://npm.acme.example/repo");
        let u = Url::parse("https://npm.acme.example/repo/@acme%2Fui").unwrap();
        assert_eq!(p.creds.for_url(&u).unwrap().header(), "Bearer SECRET-ACME");
        // The other host's token never applies to the acme host, nor the reverse.
        assert!(p.creds.for_url(&Url::parse("https://npm.acme.example/other").unwrap()).is_none());
        assert_eq!(
            p.creds.for_url(&Url::parse("https://other.example/x").unwrap()).unwrap().header(),
            "Bearer OTHER-SECRET"
        );
        assert!(p.creds.for_url(&Url::parse("https://registry.npmjs.org/x").unwrap()).is_none());
        // An unscoped package stays public.
        let q = plan(&dep("left-pad", "1.0.0"), t.path(), &env);
        assert!(!q.any_private());
    }

    #[test]
    fn user_file_env_and_basic_auth() {
        let home = tempfile::tempdir().unwrap();
        let proj = tempfile::tempdir().unwrap();
        write(
            &home.path().join(".npmrc"),
            "registry=https://mirror.example/npm\n_auth=ignored\n//mirror.example/npm/:_auth=dXNlcjpwdw==\n",
        );
        let env = Env::fixed(&[("NPM_CONFIG_@x:registry", "https://x.example/")], Some(home.path()));
        let p = plan(&dep("left-pad", "1.0.0"), proj.path(), &env);
        // A default-registry mirror is private but not definitive: public stays the fallback.
        assert!(p.any_private() && !p.definitely_private());
        assert_eq!(p.candidates.len(), 2);
        let u = Url::parse("https://mirror.example/npm/left-pad").unwrap();
        assert_eq!(p.creds.for_url(&u).unwrap().header(), Auth::Basic("user".into(), "pw".into()).header());
        let q = plan(&dep("@x/y", "1.0.0"), proj.path(), &env);
        assert_eq!(q.candidates[0].base, "https://x.example");
    }

    #[test]
    fn lockfile_resolved_url_wins() {
        let t = tempfile::tempdir().unwrap();
        write(
            &t.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/@acme/ui":{"version":"1.2.3","resolved":"https://art.acme.example/api/npm/@acme/ui/-/ui-1.2.3.tgz"}}}"#,
        );
        let p = plan(&dep("@acme/ui", "1.2.3"), t.path(), &Env::fixed(&[], None));
        assert!(p.definitely_private());
        assert_eq!(
            p.candidates[0].pinned.as_deref(),
            Some("https://art.acme.example/api/npm/@acme/ui/-/ui-1.2.3.tgz")
        );
        // A different version has no pin.
        assert!(!plan(&dep("@acme/ui", "9.9.9"), t.path(), &Env::fixed(&[], None)).any_private());
    }

    #[test]
    fn downloads_with_the_token_only_from_its_host() {
        let tarball = tgz(&[("package/README.md", b"# private"), ("package/package.json", b"{}")]);
        let other = serve(move |_, _| (200, "application/gzip".into(), tarball.clone(), vec![]));
        let other_base = other.base.clone();
        let reg = serve(move |p, auth| match (p, auth) {
            ("/ui/1.0.0", Some("Bearer SECRET-REG")) => (
                200,
                "application/json".into(),
                format!(r#"{{"dist":{{"tarball":"{other_base}/ui.tgz"}}}}"#).into_bytes(),
                vec![],
            ),
            _ => (401, "text/plain".into(), vec![], vec![]),
        });
        let t = tempfile::tempdir().unwrap();
        write(
            &t.path().join(".npmrc"),
            &format!("registry={}\n//{}/:_authToken=SECRET-REG\n", reg.base, reg.host),
        );
        let p = plan(&dep("ui", "1.0.0"), t.path(), &Env::fixed(&[], None));
        let out = tempfile::tempdir().unwrap();
        assert!(download(&Http::new(p.creds), &dep("ui", "1.0.0"), &p.candidates[0], out.path()).unwrap());
        assert!(out.path().join("README.md").is_file());
        // The tarball host is a different origin: it got no Authorization.
        let seen = other.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].1, None);
    }
}

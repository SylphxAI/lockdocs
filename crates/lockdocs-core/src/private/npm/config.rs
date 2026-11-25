//! `.npmrc` settings, origin-bound credentials, and lockfile pins.

use super::super::{b64_decode, read, Auth, Credential, Creds, Env, Origin, Url};
use crate::Dep;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const PUBLIC_HOSTS: &[&str] = &["registry.npmjs.org", "registry.yarnpkg.com"];
pub(super) const DEFAULT: &str = "https://registry.npmjs.org";

pub(super) fn is_public(url: &str) -> bool {
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
pub(super) fn config(root: &Path, env: &Env) -> BTreeMap<String, String> {
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

pub(super) fn scope(name: &str) -> Option<&str> {
    name.starts_with('@').then(|| name.split('/').next()).flatten()
}

/// Credentials for every `//host[:port]/path/:key` entry, each bound to the
/// scheme of a configured registry on that host (https otherwise).
pub(super) fn credentials(cfg: &BTreeMap<String, String>, registries: &[String]) -> Creds {
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
pub(super) fn lock_resolved(dep: &Dep, root: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&read(&root.join("package-lock.json"))?).ok()?;
    let pk = v.get("packages")?.as_object()?;
    let suffix = format!("node_modules/{}", dep.name);
    pk.iter()
        .filter(|(k, e)| {
            (k.as_str() == suffix || k.ends_with(&format!("/{suffix}"))) && e.get("version").and_then(|x| x.as_str()) == Some(dep.version.as_str())
        })
        .find_map(|(_, e)| e.get("resolved").and_then(|r| r.as_str()).filter(|r| r.starts_with("http")).map(String::from))
}

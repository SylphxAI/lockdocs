use super::super::{read, Env, Url};
use crate::{Dep, Eco};
use std::path::{Path, PathBuf};

const PUBLIC_HOSTS: &[&str] = &["crates.io", "index.crates.io", "static.crates.io", "github.com"];

pub(super) fn cargo_home(env: &Env) -> Option<PathBuf> {
    env.get("CARGO_HOME").map(PathBuf::from).or_else(|| env.home_file(".cargo"))
}

pub(super) fn table_of(p: &Path) -> Option<toml::Table> {
    toml::from_str(&read(p)?).ok()
}

/// Config tables, lowest priority first: `$CARGO_HOME`, then each ancestor
/// directory's `.cargo/`, the project's last.
pub(super) fn configs(root: &Path, env: &Env) -> Vec<toml::Table> {
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

pub(super) fn norm_index(s: &str) -> String {
    s.trim()
        .strip_prefix("sparse+")
        .or_else(|| s.trim().strip_prefix("registry+"))
        .unwrap_or(s.trim())
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn is_public(index: &str) -> bool {
    let i = norm_index(index);
    Url::parse(&i).is_none_or(|u| PUBLIC_HOSTS.contains(&u.host.as_str()) && (u.host != "github.com" || u.path.contains("crates.io-index")))
}

/// The public fetch base for a crates.io `Cargo.lock` source, or `None` when it
/// is not one. `static.crates.io` serves the `.crate` files directly.
pub(super) fn public_base(src: &str) -> Option<String> {
    is_public(src).then(|| "https://static.crates.io".to_string())
}

pub(super) struct Registry {
    pub(super) name: String,
    pub(super) index: String,
}

pub(super) fn registries(cfgs: &[toml::Table], env: &Env) -> Vec<Registry> {
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
pub(super) fn token(name: &str, cfgs: &[toml::Table], creds_files: &[toml::Table], env: &Env) -> Option<String> {
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

pub(super) fn lock_entry(dep: &Dep, root: &Path) -> Option<String> {
    let t = table_of(&root.join("Cargo.lock"))?;
    let want = crate::norm_name(Eco::Cargo, &dep.name);
    t.get("package")?.as_array()?.iter().filter_map(|p| p.as_table()).find_map(|p| {
        (crate::norm_name(Eco::Cargo, p.get("name")?.as_str()?) == want && p.get("version")?.as_str()? == dep.version).then_some(())?;
        p.get("source")?.as_str().map(String::from)
    })
}

/// The registry name a `Cargo.toml` dependency asks for.
pub(super) fn manifest_registry(dep: &Dep, root: &Path) -> Option<String> {
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

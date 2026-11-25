use super::{ancestors, global_yarn_cache, src, tilde, Source};
use crate::Dep;
use std::path::{Path, PathBuf};

fn npm_version(dir: &Path) -> Option<String> {
    let t = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&t).ok()?;
    v.get("version")?.as_str().map(String::from)
}

/// Installed version of an npm package reachable from `root`.
pub fn npm_installed(name: &str, root: &Path) -> Option<(PathBuf, String)> {
    for a in ancestors(root) {
        let d = a.join("node_modules").join(name);
        if let Some(v) = npm_version(&d) {
            return Some((d, v));
        }
    }
    None
}

pub(super) fn locate(dep: &Dep, root: &Path) -> Option<Source> {
    let mut fallback = None;
    for a in ancestors(root) {
        let nm = a.join("node_modules");
        if !nm.is_dir() {
            continue;
        }
        let d = nm.join(&dep.name);
        if let Some(v) = npm_version(&d) {
            if v == dep.version {
                return Some(src(d, &a, v));
            }
            fallback.get_or_insert((d, a.clone(), v));
        }
        // pnpm's virtual store keeps every version side by side.
        let store = nm.join(".pnpm");
        if store.is_dir() {
            let prefix = format!("{}@{}", dep.name.replace('/', "+"), dep.version);
            if let Ok(rd) = std::fs::read_dir(&store) {
                for e in rd.flatten() {
                    let f = e.file_name().to_string_lossy().to_string();
                    if f == prefix || f.starts_with(&format!("{prefix}_")) || f.starts_with(&format!("{prefix}(")) {
                        let d = e.path().join("node_modules").join(&dep.name);
                        if let Some(v) = npm_version(&d) {
                            return Some(src(d, &a, v));
                        }
                    }
                }
            }
        }
    }
    if let Some(s) = pnp(dep, root) {
        return Some(s);
    }
    let (d, a, v) = fallback?;
    Some(src(d, &a, v))
}

/// Yarn Berry cache zip name prefix: `zod-npm-3.23.8-`, `@types-node-npm-20.1.0-`.
pub fn pnp_zip_prefix(name: &str, version: &str) -> String {
    format!("{}-npm-{}-", name.replace('/', "-"), version)
}

/// Yarn caches to look in: project (and ancestors) `.yarn/cache`, then the global cache.
fn yarn_caches(root: &Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut out: Vec<(PathBuf, Option<PathBuf>)> = ancestors(root).into_iter().map(|a| (a.join(".yarn").join("cache"), Some(a))).collect();
    if let Some(g) = global_yarn_cache() {
        out.push((g, None));
    }
    out
}

/// A package from a Yarn Plug'n'Play zip cache, unpacked (docs and sources
/// only) into the lockdocs cache once.
fn pnp(dep: &Dep, root: &Path) -> Option<Source> {
    let prefix = pnp_zip_prefix(&dep.name, &dep.version);
    for (cache, base) in yarn_caches(root) {
        let Ok(rd) = std::fs::read_dir(&cache) else { continue };
        let mut zips: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().is_some_and(|f| {
                    let f = f.to_string_lossy();
                    f.starts_with(&prefix) && f.ends_with(".zip")
                })
            })
            .collect();
        zips.sort();
        let Some(zip) = zips.into_iter().next() else { continue };
        let safe = format!("{}@{}-pnp", dep.name, dep.version).replace(['/', '\\', ':'], "+");
        let dir = crate::cache::dir().join("src").join("npm").join(safe);
        let marker = dir.join(".lockdocs-complete");
        if !marker.is_file() {
            let bytes = std::fs::read(&zip).ok()?;
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).ok()?;
            let inner = format!("node_modules/{}/", dep.name);
            if crate::fetch::unzip(&bytes, &dir, false, Some(&inner)).is_err() {
                let _ = std::fs::remove_dir_all(&dir);
                continue;
            }
            std::fs::write(&marker, b"").ok()?;
        }
        let version = npm_version(&dir).unwrap_or_else(|| dep.version.clone());
        let label = match base.as_ref().and_then(|b| zip.strip_prefix(b).ok()) {
            Some(r) => r.display().to_string().replace('\\', "/"),
            None => tilde(&zip),
        };
        return Some(Source {
            dir,
            files: None,
            metadata: None,
            version,
            label,
            fetched: false,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn pnp_names() {
        assert_eq!(super::pnp_zip_prefix("zod", "3.23.8"), "zod-npm-3.23.8-");
        assert_eq!(super::pnp_zip_prefix("@types/node", "20.1.0"), "@types-node-npm-20.1.0-");
        assert!(!"zod-npm-3.23.80-abc-def.zip".starts_with(&super::pnp_zip_prefix("zod", "3.23.8")));
    }
}

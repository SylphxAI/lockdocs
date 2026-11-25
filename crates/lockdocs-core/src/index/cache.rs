use super::build::{build, types_pkg};
use super::{PackageIndex, FORMAT};
use crate::embed;
use crate::extract;
use crate::locate::Source;
use crate::upstream::Manifest;
use crate::{cache, Dep, Eco};
use std::path::{Path, PathBuf};

/// Files whose change means the package was modified in place.
fn stamp(src: &Source) -> String {
    let probe: Vec<PathBuf> = match &src.metadata {
        Some(m) => vec![m.clone(), m.with_file_name("RECORD")],
        None => ["package.json", "Cargo.toml", "go.mod", ".lockdocs-complete"]
            .iter()
            .map(|f| src.dir.join(f))
            .collect(),
    };
    let mut s = String::new();
    for p in probe {
        if let Ok(m) = std::fs::metadata(&p) {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            s.push_str(&format!("{}:{t};", m.len()));
        }
    }
    s
}

pub(crate) fn source_stamp(dep: &Dep, src: &Source, root: &Path) -> String {
    npm_source_stamp(dep, src, types_pkg(dep, root).as_deref())
}

fn npm_source_stamp(dep: &Dep, src: &Source, types: Option<&Path>) -> String {
    if dep.eco != Eco::Npm {
        return stamp(src);
    }
    cache::hash(&[
        &std::fs::read_to_string(src.dir.join("package.json")).unwrap_or_default(),
        &extract::npm_stamp(src, types),
        &types.and_then(|t| std::fs::read_to_string(t.join("package.json")).ok()).unwrap_or_default(),
    ])
}

fn cache_path(dep: &Dep, src: &Source, types: Option<&Path>, up: Option<&(PathBuf, Manifest)>, embed: &str) -> PathBuf {
    let up_key = up
        .map(|(_, m)| {
            format!(
                "{}@{:?}:{}:{:?}:{:?}:{:?}:{}:{}",
                m.repo, m.tag, m.files, m.site, m.commit, m.note, m.docs_sites_checked, m.format
            )
        })
        .unwrap_or_default();
    let key = cache::hash(&[
        embed,
        &up_key,
        &FORMAT.to_string(),
        dep.eco.as_str(),
        &dep.name,
        &src.version,
        &src.dir.to_string_lossy(),
        &npm_source_stamp(dep, src, types),
        &types.map(|t| t.to_string_lossy().to_string()).unwrap_or_default(),
    ]);
    let safe = format!("{}@{}", dep.name, src.version).replace(['/', '\\', ':'], "+");
    cache::dir().join("idx").join(dep.eco.as_str()).join(format!("{safe}-{key}.bin"))
}

/// Load from the disk cache, or build and store.
pub fn load_or_build(dep: &Dep, src: &Source, root: &Path, up: Option<&(PathBuf, Manifest)>) -> PackageIndex {
    let types = types_pkg(dep, root);
    let embed_id = if embed::available() { embed::MODEL_ID } else { "" };
    let path = cache_path(dep, src, types.as_deref(), up, embed_id);
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(idx) = postcard::from_bytes::<PackageIndex>(&bytes) {
            if idx.format == FORMAT {
                return idx;
            }
        }
    }
    let idx = build(dep, src, root, up);
    if let Ok(bytes) = postcard::to_stdvec(&idx) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
    idx
}

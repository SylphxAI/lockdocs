use super::terms::{embed_text, entry_tf, head_tf};
use super::{PackageIndex, FORMAT};
use crate::embed::{self, Vec8};
use crate::extract::{self, Kind};
use crate::locate::{self, Source};
use crate::upstream::Manifest;
use crate::{Dep, Eco};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// `@types/<name>` next to an npm package that ships no declarations.
pub(super) fn types_pkg(dep: &Dep, root: &Path) -> Option<PathBuf> {
    if dep.eco != Eco::Npm || dep.name.starts_with("@types/") {
        return None;
    }
    let mangled = match dep.name.strip_prefix('@') {
        Some(scoped) => scoped.replacen('/', "__", 1),
        None => dep.name.clone(),
    };
    locate::npm_installed(&format!("@types/{mangled}"), root).map(|(d, _)| d)
}

pub fn build(dep: &Dep, src: &Source, root: &Path, up: Option<&(PathBuf, Manifest)>) -> PackageIndex {
    let t = Instant::now();
    let types = types_pkg(dep, root);
    let up_dir = up.filter(|(_, m)| m.files > 0).map(|(d, _)| d.as_path());
    let mut entries = extract::extract(dep.eco, &dep.name, src, types.as_deref(), up_dir);
    let npm_api = (dep.eco == Eco::Npm).then(|| {
        let sources = extract::npm_sources(src, types.as_deref());
        // Separate @types packages keep their real citation paths, but their
        // own package.json supplies the declaration entrypoint.
        if let Some(types) = &types {
            let prefix = types
                .iter()
                .rev()
                .take(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
                + "/";
            if sources.keys().any(|f| f.starts_with(&prefix)) {
                let relative = sources
                    .iter()
                    .filter_map(|(f, s)| f.strip_prefix(&prefix).map(|f| (f.to_string(), s.clone())))
                    .collect();
                return crate::npm_api::PublicApi::build(&dep.name, types, &relative).prefixed(&prefix);
            }
        }
        crate::npm_api::PublicApi::build(&dep.name, &src.dir, &sources)
    });
    if let Some(api) = &npm_api {
        entries.retain(|e| e.kind == Kind::Prose);
        // Keep exact declaration identities, including .d.cts/.d.mts. The
        // requested import selects and deduplicates its own search corpus.
        entries.extend(api.entries.iter().cloned());
    }
    let (terms, lens): (Vec<_>, Vec<_>) = entries.iter().map(entry_tf).unzip();
    let (head, head_lens): (Vec<_>, Vec<_>) = entries.iter().map(head_tf).unzip();
    let model = embed::get();
    let vecs: Vec<Vec8> = match &model {
        Some(m) => {
            use rayon::prelude::*;
            entries.par_iter().map(|e| m.embed8(&embed_text(e)).unwrap_or_default()).collect()
        }
        None => Vec::new(),
    };
    PackageIndex {
        format: FORMAT,
        eco: dep.eco,
        name: dep.name.clone(),
        version: src.version.clone(),
        source: src.label.clone(),
        fetched: src.fetched,
        entries,
        npm_api,
        terms,
        lens,
        head,
        head_lens,
        build_ms: t.elapsed().as_millis() as u64,
        upstream: up.filter(|(_, m)| m.files > 0).map(|(_, m)| {
            format!(
                "{}@{}{}{} ({} files)",
                m.repo,
                m.tag.as_deref().unwrap_or("?"),
                m.commit.as_ref().map(|c| format!(" commit {c}")).unwrap_or_default(),
                m.site.as_ref().map(|s| format!(" + docs site {s}")).unwrap_or_default(),
                m.files
            )
        }),
        upstream_provenance: up.map(|(_, m)| m.clone()),
        embed: if model.is_some() { embed::MODEL_ID.to_string() } else { String::new() },
        vecs,
    }
}

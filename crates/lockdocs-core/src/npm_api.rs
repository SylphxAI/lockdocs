//! Public npm declaration graph. Names are resolved through package entrypoints,
//! not by scoring unrelated declarations which happen to share a short name.

mod module;
mod resolve;

use self::module::{leaves, module, resolve_file, Binding, Module};
use crate::extract::Entry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Default, Serialize, Deserialize)]
pub struct PublicApi {
    pub roots: Vec<Root>,
    pub entries: Vec<Entry>,
    modules: BTreeMap<String, Module>,
    legacy_subpaths: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Root {
    pub import: String,
    pub conditions: String,
    pub file: String,
}

#[derive(Default)]
pub struct Resolution {
    pub entries: BTreeSet<usize>,
    pub unresolved: bool,
}

impl PublicApi {
    pub fn build(name: &str, dir: &Path, sources: &BTreeMap<String, String>) -> Self {
        let files: BTreeSet<String> = sources.keys().cloned().collect();
        let modules = sources.iter().map(|(f, s)| (f.clone(), module(s, f, &files))).collect();
        let pkg: Value = std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        let mut paths = BTreeMap::new();
        if let Some(exports) = pkg.get("exports") {
            if let Some(o) = exports.as_object().filter(|o| o.keys().any(|k| k.starts_with('.'))) {
                for (k, v) in o {
                    paths.insert(k.clone(), v.clone());
                }
            } else {
                paths.insert(".".into(), exports.clone());
            }
        } else {
            let target = pkg
                .get("types")
                .or_else(|| pkg.get("typings"))
                .or_else(|| pkg.get("main"))
                .and_then(Value::as_str)
                .unwrap_or("./index.d.ts");
            paths.insert(".".into(), Value::String(format!("./{}", target.trim_start_matches("./"))));
        }
        let routes: Vec<String> = paths.keys().cloned().collect();
        let mut roots = Vec::new();
        for (path, value) in paths {
            if path == "./package.json" {
                continue;
            }
            let mut targets = Vec::new();
            leaves(&value, "", &mut targets);
            for (conditions, target) in targets {
                if let (Some((pre, post)), Some((tp, ts))) = (path.split_once('*'), target.split_once('*')) {
                    for file in &files {
                        if let Some(mid) = file.strip_prefix(tp.trim_start_matches("./")).and_then(|s| s.strip_suffix(ts)) {
                            let public = format!("{pre}{mid}{post}");
                            let best = routes
                                .iter()
                                .filter(|r| {
                                    if let Some((a, b)) = r.split_once('*') {
                                        public.starts_with(a) && public.ends_with(b)
                                    } else {
                                        **r == public
                                    }
                                })
                                .max_by_key(|r| {
                                    if let Some((a, b)) = r.split_once('*') {
                                        (0, a.len(), b.len())
                                    } else {
                                        (1, r.len(), 0)
                                    }
                                });
                            if best != Some(&path) {
                                continue;
                            }
                            roots.push(Root {
                                import: format!("{name}{}{}{}", pre.trim_start_matches('.'), mid, post),
                                conditions: conditions.clone(),
                                file: file.clone(),
                            });
                        }
                    }
                } else {
                    let file = resolve_file("", &format!("./{}", target.trim_start_matches("./")), &files);
                    roots.push(Root {
                        import: format!("{name}{}", path.trim_start_matches('.')),
                        conditions,
                        file,
                    });
                }
            }
        }
        let legacy_subpaths = pkg.get("exports").is_none();
        if legacy_subpaths {
            // Without an exports map, package files remain importable through
            // legacy subpaths. Keep those routes available to explicit API
            // queries as well as package-wide documentation search.
            for file in &files {
                let stem = [".d.ts", ".d.cts", ".d.mts", ".tsx", ".ts", ".cts", ".mts", ".js", ".cjs", ".mjs"]
                    .iter()
                    .find_map(|suffix| file.strip_suffix(suffix));
                if let Some(stem) = stem {
                    for path in [Some(stem), stem.strip_suffix("/index")].into_iter().flatten() {
                        roots.push(Root {
                            import: format!("{name}/{path}"),
                            conditions: "legacy".into(),
                            file: file.clone(),
                        });
                    }
                }
            }
        }
        Self {
            roots,
            entries: crate::extract::npm_entries(sources),
            modules,
            legacy_subpaths,
        }
    }

    /// Rebase an @types declaration graph onto the extraction's citation labels.
    pub(crate) fn prefixed(mut self, prefix: &str) -> Self {
        let rebase = |s: &mut String| {
            if !s.starts_with("external:") {
                *s = format!("{prefix}{s}");
            }
        };
        let binding = |b: &mut Binding| match b {
            Binding::Named(file, _) | Binding::Namespace(file) => rebase(file),
            _ => (),
        };
        for e in &mut self.entries {
            rebase(&mut e.file);
        }
        for root in &mut self.roots {
            rebase(&mut root.file);
        }
        self.modules = self
            .modules
            .into_iter()
            .map(|(file, mut m)| {
                for b in m.exports.values_mut().chain(m.imports.values_mut()) {
                    binding(b);
                }
                for star in &mut m.stars {
                    rebase(star);
                }
                (format!("{prefix}{file}"), m)
            })
            .collect();
        self
    }
}

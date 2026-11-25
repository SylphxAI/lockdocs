//! Normalize package selectors and narrow documentation to applicable public imports.

use super::super::entries::npm_import_base;
use super::super::packages::ReadyPkg;
use crate::index::PackageIndex;
use crate::project::Spec;
use crate::{Dep, Eco};
use std::collections::{BTreeSet, HashMap, HashSet};

type Imports = HashMap<String, Vec<String>>;

pub(super) fn normalize_selector(package: Option<&str>, deps: &[Dep]) -> (Option<String>, Imports) {
    let mut imports = HashMap::new();
    let selector = package.map(|p| {
        p.split(',')
            .map(|p| {
                let spec = Spec::parse(p.trim());
                let base = npm_import_base(&spec.name);
                let npm = spec.eco == Some(Eco::Npm) || spec.eco.is_none() && deps.iter().any(|d| d.eco == Eco::Npm && d.name == base);
                if npm && base != spec.name {
                    imports.entry(base.to_string()).or_insert_with(Vec::new).push(spec.name.clone());
                    format!("npm:{base}{}", spec.version.as_ref().map(|v| format!("@{v}")).unwrap_or_default())
                } else {
                    if npm {
                        imports.entry(base.to_string()).or_insert_with(Vec::new).push(base.to_string());
                    }
                    p.trim().to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    });
    (selector, imports)
}

pub(super) struct SearchScope {
    pub(super) scopes: Vec<BTreeSet<usize>>,
    pub(super) public_calls: HashSet<(usize, usize)>,
}

pub(super) fn search_scope(ready: &[ReadyPkg], imports: &Imports, query: &str) -> Result<SearchScope, String> {
    let mut scopes = Vec::new();
    let mut public_calls = HashSet::new();
    for (pi, (idx, _, _)) in ready.iter().enumerate() {
        let mut requested = imports.get(&idx.name).cloned().unwrap_or_else(|| vec![idx.name.clone()]);
        // A question naming known public imports can narrow a root search,
        // but must never override an explicitly selected alternate edition.
        if requested.iter().all(|import| import == &idx.name) {
            if let Some(api) = &idx.npm_api {
                let named: BTreeSet<String> = api
                    .roots
                    .iter()
                    .filter(|r| r.import != idx.name && mentions_import(query, &r.import))
                    .map(|r| r.import.clone())
                    .collect();
                if !named.is_empty() {
                    requested = named.into_iter().collect();
                }
            }
        }
        if let Some(api) = &idx.npm_api {
            for import in &requested {
                if import != &idx.name && !api.roots.iter().any(|r| &r.import == import) {
                    return Err(format!("`{import}` has no public declaration entrypoint in {}.", idx.id()));
                }
            }
        }
        let mut scope = BTreeSet::new();
        for import in requested {
            scope.extend(idx.docs_indices(Some(&import)));
            public_calls.extend(public_call_entries(idx, &import, query).into_iter().map(|ei| (pi, ei)));
        }
        scopes.push(scope);
    }
    Ok(SearchScope { scopes, public_calls })
}

/// Exact case-sensitive module token, not a prefix or package-looking substring.
fn mentions_import(query: &str, import: &str) -> bool {
    let module_char = |c: char| c.is_alphanumeric() || matches!(c, '@' | '/' | '_' | '-' | '.');
    query.match_indices(import).any(|(i, _)| {
        let before = query[..i].chars().next_back();
        let after = &query[i + import.len()..];
        before.is_none_or(|c| !module_char(c))
            && (after.chars().next().is_none_or(|c| !module_char(c)) || after.starts_with('.') && after[1..].chars().next().is_none_or(|c| !module_char(c)))
    })
}

/// Resolve explicit calls through the selected public entrypoint, rather than
/// treating every same-named member in its transitive type graph as the API.
fn public_call_entries(idx: &PackageIndex, import: &str, query: &str) -> Vec<usize> {
    let Some(api) = &idx.npm_api else {
        return Vec::new();
    };
    let names: HashSet<&str> = api
        .entries
        .iter()
        .flat_map(|e| [e.name.as_str(), e.path.as_str()])
        .filter(|name| {
            query.match_indices(*name).any(|(i, _)| {
                query[i + name.len()..].starts_with('(')
                    && query[..i]
                        .chars()
                        .next_back()
                        .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | ':' | '/' | '-' | '@')))
            })
        })
        .collect();
    let roots: Vec<_> = api.roots.iter().filter(|r| r.import == import).collect();
    let mut targets = HashSet::new();
    for name in names {
        let resolved: Vec<_> = roots.iter().map(|root| api.resolve(root, name, &api.entries)).collect();
        let Some(first) = resolved.first() else {
            continue;
        };
        if first.entries.is_empty() || resolved.iter().any(|r| r.unresolved || r.entries != first.entries) {
            continue;
        }
        for &i in &first.entries {
            let e = &api.entries[i];
            targets.insert((e.file.as_str(), e.path.as_str(), e.line));
        }
    }
    idx.entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| targets.contains(&(e.file.as_str(), e.path.as_str(), e.line)).then_some(i))
        .collect()
}

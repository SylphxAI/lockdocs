//! Cargo alternate registries (sparse protocol), from `.cargo/config.toml`,
//! `$CARGO_HOME/credentials.toml`, `CARGO_REGISTRIES_<NAME>_*` variables, the
//! registry `Cargo.lock` recorded per package and the `registry = "name"` key
//! of `Cargo.toml` dependencies.

mod config;
mod download;

#[cfg(test)]
mod tests;

pub use download::{dl_url, download};

use super::{Auth, Candidate, Credential, Creds, Env, Plan, Url};
use crate::Dep;
use config::{cargo_home, configs, is_public, lock_entry, manifest_registry, norm_index, public_base, registries, table_of, token};
use std::path::Path;

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
        } else if let Some(base) = public_base(&src) {
            // Cargo.lock resolved this package to crates.io: honour it. A
            // configured mirror or replacement registry must not shadow the
            // artifact the lockfile actually pinned.
            candidates.push(Candidate {
                base,
                private: false,
                definitive: true,
                pinned: None,
            });
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

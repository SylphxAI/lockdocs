use super::dep;
use crate::{Dep, Eco};
use std::collections::HashSet;

pub fn cargo_lock(text: &str) -> anyhow::Result<Vec<Dep>> {
    let v: toml::Table = toml::from_str(text)?;
    let pkgs = v.get("package").and_then(|p| p.as_array()).map(Vec::as_slice).unwrap_or_default();
    let mut direct = HashSet::new();
    for p in pkgs {
        if p.get("source").is_none() {
            for d in p.get("dependencies").and_then(|d| d.as_array()).into_iter().flatten() {
                if let Some(s) = d.as_str() {
                    direct.insert(s.split(' ').next().unwrap_or(s).to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    for p in pkgs {
        let (Some(name), Some(ver)) = (p.get("name").and_then(|n| n.as_str()), p.get("version").and_then(|n| n.as_str())) else {
            continue;
        };
        let Some(src) = p.get("source").and_then(|s| s.as_str()) else { continue };
        let from = if src.starts_with("registry+") || src.starts_with("sparse+") {
            "Cargo.lock"
        } else if src.starts_with("git+") {
            "Cargo.lock (git)"
        } else {
            continue;
        };
        out.push(dep(Eco::Cargo, name, ver, direct.contains(name), from));
    }
    Ok(out)
}

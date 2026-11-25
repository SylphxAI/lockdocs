use super::{dep, git_url};
use crate::{Dep, Eco};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Direct dependency names declared in `package.json`.
pub fn package_json_direct(dir: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    if let Ok(t) = std::fs::read_to_string(dir.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&t) {
            collect_dep_keys(&v, &mut out);
        }
    }
    out
}

fn collect_dep_keys(v: &Value, out: &mut HashSet<String>) {
    for k in ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(m) = v.get(k).and_then(|m| m.as_object()) {
            out.extend(m.keys().cloned());
        }
    }
}

pub fn npm_lock(text: &str, from: &str) -> anyhow::Result<Vec<Dep>> {
    let v: Value = serde_json::from_str(text)?;
    let mut out = Vec::new();
    if let Some(pkgs) = v.get("packages").and_then(|p| p.as_object()) {
        let mut direct = HashSet::new();
        if let Some(root) = pkgs.get("") {
            collect_dep_keys(root, &mut direct);
        }
        // Workspace members declare direct deps too.
        for (k, p) in pkgs {
            if !k.is_empty() && !k.contains("node_modules/") {
                collect_dep_keys(p, &mut direct);
            }
        }
        for (k, p) in pkgs {
            let Some(name) = k.strip_prefix("node_modules/") else { continue };
            if name.contains("/node_modules/") || p.get("link").and_then(|l| l.as_bool()) == Some(true) {
                continue;
            }
            let Some(ver) = p.get("version").and_then(|v| v.as_str()) else { continue };
            let name = p.get("name").and_then(|n| n.as_str()).unwrap_or(name);
            let origin = if p.get("resolved").and_then(Value::as_str).is_some_and(git_url) || git_url(ver) {
                format!("{from} (git)")
            } else {
                from.to_string()
            };
            out.push(dep(Eco::Npm, name, ver, direct.contains(name), &origin));
        }
    } else if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
        // lockfileVersion 1
        for (name, p) in deps {
            if let Some(ver) = p.get("version").and_then(|v| v.as_str()) {
                let origin = if p.get("resolved").and_then(Value::as_str).is_some_and(git_url) || git_url(ver) {
                    format!("{from} (git)")
                } else {
                    from.to_string()
                };
                out.push(dep(Eco::Npm, name, ver, true, &origin));
            }
        }
        // v1 has no reliable direct marker: every top-level entry counts.
    }
    Ok(out)
}

fn strip_peer_suffix(v: &str) -> &str {
    v.split('(').next().unwrap_or(v).trim()
}

/// Split `name@version` where the name may be scoped (`@a/b@1.0.0`).
fn split_at_version(s: &str) -> Option<(&str, &str)> {
    let start = if s.starts_with('@') { 1 } else { 0 };
    let at = s[start..].find('@')? + start;
    Some((&s[..at], &s[at + 1..]))
}

pub fn pnpm_lock(text: &str) -> anyhow::Result<Vec<Dep>> {
    use yaml_rust2::{Yaml, YamlLoader};
    let docs = YamlLoader::load_from_str(text)?;
    let Some(doc) = docs.first() else { return Ok(Vec::new()) };
    let mut direct: BTreeMap<String, String> = BTreeMap::new();
    let mut read_importer = |imp: &Yaml| {
        for sect in ["dependencies", "devDependencies", "optionalDependencies"] {
            if let Some(h) = imp[sect].as_hash() {
                for (k, v) in h {
                    let Some(name) = k.as_str() else { continue };
                    let ver = match v {
                        Yaml::String(s) => s.clone(),
                        Yaml::Real(s) => s.clone(),
                        Yaml::Hash(_) => v["version"].as_str().map(String::from).unwrap_or_else(|| match &v["version"] {
                            Yaml::Real(s) => s.clone(),
                            _ => String::new(),
                        }),
                        _ => String::new(),
                    };
                    let ver = strip_peer_suffix(&ver).to_string();
                    if ver.starts_with("link:") || ver.starts_with("file:") || ver.is_empty() {
                        continue;
                    }
                    direct.entry(name.to_string()).or_insert(ver);
                }
            }
        }
    };
    if let Some(imps) = doc["importers"].as_hash() {
        for (_, imp) in imps {
            read_importer(imp);
        }
    } else {
        read_importer(doc);
    }
    let mut out: Vec<Dep> = Vec::new();
    let mut seen = HashSet::new();
    for (name, ver) in &direct {
        // v5/v6 importers sometimes carry `/name/1.2.3` style versions.
        let ver = ver.rsplit('/').next().unwrap_or(ver);
        if seen.insert(format!("{name}@{ver}")) {
            out.push(dep(Eco::Npm, name, ver, true, "pnpm-lock.yaml"));
        }
    }
    let pkgs = doc["packages"].as_hash().or_else(|| doc["snapshots"].as_hash());
    if let Some(pkgs) = pkgs {
        for (k, _) in pkgs {
            let Some(key) = k.as_str() else { continue };
            let key = strip_peer_suffix(key.trim_start_matches('/'));
            // v9/v6: name@version; v5: name/version
            let parsed = split_at_version(key).or_else(|| {
                let i = key.rfind('/')?;
                Some((&key[..i], &key[i + 1..]))
            });
            let Some((name, ver)) = parsed else { continue };
            let ver = strip_peer_suffix(ver);
            if ver.contains(':') || name.is_empty() {
                continue;
            }
            if seen.insert(format!("{name}@{ver}")) {
                out.push(dep(Eco::Npm, name, ver, false, "pnpm-lock.yaml"));
            }
        }
    }
    Ok(out)
}

/// Remove trailing commas so bun's JSONC lockfile parses as JSON.
fn strip_trailing_commas(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut in_str = false;
    let mut esc = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
        } else if c == ',' {
            let mut j = i + 1;
            while j < b.len() && (b[j] as char).is_whitespace() {
                j += 1;
            }
            if j < b.len() && (b[j] == b'}' || b[j] == b']') {
                i += 1;
                continue;
            }
        }
        // Push the full UTF-8 char starting at i.
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

pub fn bun_lock(text: &str) -> anyhow::Result<Vec<Dep>> {
    let v: Value = serde_json::from_str(&strip_trailing_commas(text))?;
    let mut direct = HashSet::new();
    if let Some(ws) = v.get("workspaces").and_then(|w| w.as_object()) {
        for (_, w) in ws {
            collect_dep_keys(w, &mut direct);
        }
    }
    let mut out = Vec::new();
    if let Some(pkgs) = v.get("packages").and_then(|p| p.as_object()) {
        for (key, entry) in pkgs {
            // Nested keys (`a/b`) are non-hoisted duplicates, except scoped names.
            let nested = if key.starts_with('@') {
                key.matches('/').count() > 1
            } else {
                key.contains('/')
            };
            if nested {
                continue;
            }
            let Some(spec) = entry.get(0).and_then(|s| s.as_str()) else { continue };
            let Some((name, ver)) = split_at_version(spec) else { continue };
            if ver.contains(':') {
                // workspace:, link:, github: ...
                continue;
            }
            out.push(dep(Eco::Npm, name, ver, direct.contains(name), "bun.lock"));
        }
    }
    Ok(out)
}

pub fn yarn_lock(text: &str, direct: &HashSet<String>) -> Vec<Dep> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut names: Vec<String> = Vec::new();
    let mut git_origin = false;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') {
            names.clear();
            if line.starts_with("__metadata") {
                continue;
            }
            let header = line.trim_end_matches(':');
            git_origin = git_url(header);
            for spec in header.split(", ") {
                let spec = spec.trim().trim_matches('"');
                if let Some((name, _)) = split_at_version(spec) {
                    if !names.iter().any(|n| n == name) {
                        names.push(name.to_string());
                    }
                }
            }
            continue;
        }
        let t = line.trim();
        let ver = t.strip_prefix("version ").or_else(|| t.strip_prefix("version: "));
        if let Some(ver) = ver {
            let ver = ver.trim().trim_matches('"');
            for n in &names {
                if seen.insert(format!("{n}@{ver}")) {
                    out.push(dep(
                        Eco::Npm,
                        n,
                        ver,
                        direct.contains(n),
                        if git_origin { "yarn.lock (git)" } else { "yarn.lock" },
                    ));
                }
            }
            names.clear();
        }
    }
    out
}

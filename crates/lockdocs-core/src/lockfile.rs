//! Lockfile and manifest parsers. Each parser is a pure function from file
//! contents to exact versions, so they are easy to test.

use crate::{norm_name, Dep, Eco};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Lockfiles lockdocs reads, in priority order within an ecosystem.
pub const LOCKFILES: &[(&str, Eco)] = &[
    ("package-lock.json", Eco::Npm),
    ("npm-shrinkwrap.json", Eco::Npm),
    ("pnpm-lock.yaml", Eco::Npm),
    ("bun.lock", Eco::Npm),
    ("yarn.lock", Eco::Npm),
    ("Cargo.lock", Eco::Cargo),
    ("uv.lock", Eco::PyPI),
    ("poetry.lock", Eco::PyPI),
    ("pdm.lock", Eco::PyPI),
    ("Pipfile.lock", Eco::PyPI),
    ("go.mod", Eco::Go),
];

fn dep(eco: Eco, name: &str, version: &str, direct: bool, from: &str) -> Dep {
    Dep {
        eco,
        name: name.to_string(),
        version: version.trim().to_string(),
        direct,
        from: from.to_string(),
    }
}

/// Parse one lockfile by file name. Returns `None` for unknown names.
pub fn parse(file_name: &str, text: &str, dir: &Path) -> Option<anyhow::Result<Vec<Dep>>> {
    let r = match file_name {
        "package-lock.json" | "npm-shrinkwrap.json" => npm_lock(text, file_name),
        "pnpm-lock.yaml" => pnpm_lock(text),
        "bun.lock" => bun_lock(text),
        "yarn.lock" => Ok(yarn_lock(text, &package_json_direct(dir))),
        "Cargo.lock" => cargo_lock(text),
        "uv.lock" => uv_lock(text),
        "poetry.lock" | "pdm.lock" => poetry_lock(text, file_name, &pyproject_direct(dir)),
        "Pipfile.lock" => pipfile_lock(text),
        "go.mod" => Ok(go_mod(text)),
        n if is_requirements(n) => Ok(requirements(text, n)),
        _ => return None,
    };
    Some(r)
}

pub fn is_requirements(name: &str) -> bool {
    name.starts_with("requirements") && name.ends_with(".txt")
}

// ---------------------------------------------------------------- npm

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
            out.push(dep(Eco::Npm, name, ver, direct.contains(name), from));
        }
    } else if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
        // lockfileVersion 1
        for (name, p) in deps {
            if let Some(ver) = p.get("version").and_then(|v| v.as_str()) {
                out.push(dep(Eco::Npm, name, ver, true, from));
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
                    out.push(dep(Eco::Npm, n, ver, direct.contains(n), "yarn.lock"));
                }
            }
            names.clear();
        }
    }
    out
}

// ---------------------------------------------------------------- cargo

pub fn cargo_lock(text: &str) -> anyhow::Result<Vec<Dep>> {
    let v: toml::Table = toml::from_str(text)?;
    let pkgs = v.get("package").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    let mut direct = HashSet::new();
    for p in &pkgs {
        if p.get("source").is_none() {
            for d in p.get("dependencies").and_then(|d| d.as_array()).into_iter().flatten() {
                if let Some(s) = d.as_str() {
                    direct.insert(s.split(' ').next().unwrap_or(s).to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    for p in &pkgs {
        let (Some(name), Some(ver)) = (p.get("name").and_then(|n| n.as_str()), p.get("version").and_then(|n| n.as_str())) else {
            continue;
        };
        let Some(src) = p.get("source").and_then(|s| s.as_str()) else { continue };
        if !src.starts_with("registry+") && !src.starts_with("sparse+") {
            continue;
        }
        out.push(dep(Eco::Cargo, name, ver, direct.contains(name), "Cargo.lock"));
    }
    Ok(out)
}

// ---------------------------------------------------------------- python

/// Requirement names from a PEP 508 string such as `pydantic[email]>=2`.
fn req_name(s: &str) -> Option<String> {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some(norm_name(Eco::PyPI, &s[..end]))
}

/// Direct dependency names from `pyproject.toml` (PEP 621, Poetry, PDM, uv groups).
pub fn pyproject_direct(dir: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(t) = std::fs::read_to_string(dir.join("pyproject.toml")) else {
        return out;
    };
    let Ok(v) = toml::from_str::<toml::Table>(&t) else { return out };
    let mut arr = |a: Option<&toml::Value>| {
        for s in a.and_then(|a| a.as_array()).into_iter().flatten().filter_map(|x| x.as_str()) {
            out.extend(req_name(s));
        }
    };
    arr(v.get("project").and_then(|p| p.get("dependencies")));
    if let Some(opt) = v.get("project").and_then(|p| p.get("optional-dependencies")).and_then(|o| o.as_table()) {
        for (_, a) in opt {
            arr(Some(a));
        }
    }
    if let Some(groups) = v.get("dependency-groups").and_then(|o| o.as_table()) {
        for (_, a) in groups {
            arr(Some(a));
        }
    }
    let poetry = v.get("tool").and_then(|t| t.get("poetry"));
    for k in ["dependencies", "dev-dependencies"] {
        if let Some(m) = poetry.and_then(|p| p.get(k)).and_then(|m| m.as_table()) {
            out.extend(m.keys().map(|k| norm_name(Eco::PyPI, k)));
        }
    }
    if let Some(groups) = poetry.and_then(|p| p.get("group")).and_then(|g| g.as_table()) {
        for (_, g) in groups {
            if let Some(m) = g.get("dependencies").and_then(|m| m.as_table()) {
                out.extend(m.keys().map(|k| norm_name(Eco::PyPI, k)));
            }
        }
    }
    out.remove("python");
    out
}

pub fn uv_lock(text: &str) -> anyhow::Result<Vec<Dep>> {
    let v: toml::Table = toml::from_str(text)?;
    let pkgs = v.get("package").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    let mut direct = HashSet::new();
    let mut local = HashSet::new();
    for p in &pkgs {
        let src = p.get("source");
        let is_local = src.is_some_and(|s| s.get("editable").is_some() || s.get("virtual").is_some() || s.get("directory").is_some());
        if is_local {
            local.insert(p.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string());
            let mut add = |a: Option<&toml::Value>| {
                for d in a.and_then(|a| a.as_array()).into_iter().flatten() {
                    if let Some(n) = d.get("name").and_then(|n| n.as_str()) {
                        direct.insert(norm_name(Eco::PyPI, n));
                    }
                }
            };
            add(p.get("dependencies"));
            for k in ["optional-dependencies", "dev-dependencies"] {
                if let Some(t) = p.get(k).and_then(|t| t.as_table()) {
                    for (_, a) in t {
                        add(Some(a));
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for p in &pkgs {
        let (Some(name), Some(ver)) = (p.get("name").and_then(|n| n.as_str()), p.get("version").and_then(|n| n.as_str())) else {
            continue;
        };
        if local.contains(name) {
            continue;
        }
        out.push(dep(Eco::PyPI, name, ver, direct.contains(&norm_name(Eco::PyPI, name)), "uv.lock"));
    }
    Ok(out)
}

pub fn poetry_lock(text: &str, from: &str, direct: &HashSet<String>) -> anyhow::Result<Vec<Dep>> {
    let v: toml::Table = toml::from_str(text)?;
    let mut out = Vec::new();
    for p in v.get("package").and_then(|p| p.as_array()).into_iter().flatten() {
        let (Some(name), Some(ver)) = (p.get("name").and_then(|n| n.as_str()), p.get("version").and_then(|n| n.as_str())) else {
            continue;
        };
        let is_direct = direct.is_empty() || direct.contains(&norm_name(Eco::PyPI, name));
        out.push(dep(Eco::PyPI, name, ver, is_direct, from));
    }
    Ok(out)
}

pub fn pipfile_lock(text: &str) -> anyhow::Result<Vec<Dep>> {
    let v: Value = serde_json::from_str(text)?;
    let mut out = Vec::new();
    for sect in ["default", "develop"] {
        for (name, p) in v.get(sect).and_then(|s| s.as_object()).into_iter().flatten() {
            if let Some(ver) = p.get("version").and_then(|v| v.as_str()) {
                out.push(dep(Eco::PyPI, name, ver.trim_start_matches("=="), true, "Pipfile.lock"));
            }
        }
    }
    Ok(out)
}

/// Pinned lines (`name==1.2.3`) from a requirements file.
pub fn requirements(text: &str, from: &str) -> Vec<Dep> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split(" #").next().unwrap_or(line).trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        let line = line.split(';').next().unwrap_or(line);
        let Some((lhs, ver)) = line.split_once("==") else { continue };
        let name = lhs.split('[').next().unwrap_or(lhs).trim();
        let ver = ver.trim().split([',', ' ', '\\']).next().unwrap_or("").trim_start_matches('=');
        if !name.is_empty() && !ver.is_empty() {
            out.push(dep(Eco::PyPI, name, ver, true, from));
        }
    }
    out
}

// ---------------------------------------------------------------- go

pub fn go_mod(text: &str) -> Vec<Dep> {
    let mut out = Vec::new();
    let mut in_block = false;
    let mut replaced: BTreeMap<String, String> = BTreeMap::new();
    let mut in_replace = false;
    for raw in text.lines() {
        let line = raw.trim();
        if in_replace {
            if line == ")" {
                in_replace = false;
            } else {
                parse_replace(line, &mut replaced);
            }
            continue;
        }
        if let Some(r) = line.strip_prefix("replace ") {
            if r.trim() == "(" {
                in_replace = true;
            } else {
                parse_replace(r, &mut replaced);
            }
            continue;
        }
        let spec = if in_block {
            if line == ")" {
                in_block = false;
                continue;
            }
            line
        } else if let Some(r) = line.strip_prefix("require ") {
            if r.trim() == "(" {
                in_block = true;
                continue;
            }
            r
        } else {
            continue;
        };
        let indirect = spec.contains("// indirect");
        let spec = spec.split("//").next().unwrap_or(spec).trim();
        let mut parts = spec.split_whitespace();
        let (Some(m), Some(v)) = (parts.next(), parts.next()) else { continue };
        out.push(dep(Eco::Go, m, v, !indirect, "go.mod"));
    }
    for d in &mut out {
        if let Some(v) = replaced.get(&d.name) {
            d.version = v.clone();
        }
    }
    out
}

/// `old [v] => new v` where new is the same module: keep the version override.
fn parse_replace(line: &str, out: &mut BTreeMap<String, String>) {
    let Some((lhs, rhs)) = line.split_once("=>") else { return };
    let old = lhs.split_whitespace().next().unwrap_or("");
    let mut r = rhs.split_whitespace();
    if let (Some(new), Some(ver)) = (r.next(), r.next()) {
        if new == old {
            out.insert(old.to_string(), ver.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_v3() {
        let t = r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"zod":"^3.23.0"},"devDependencies":{"@types/node":"^20"}},
          "node_modules/zod":{"version":"3.23.8"},
          "node_modules/@types/node":{"version":"20.1.0","dev":true},
          "node_modules/a/node_modules/zod":{"version":"3.0.0"},
          "node_modules/ws-pkg":{"resolved":"packages/x","link":true}}}"#;
        let d = npm_lock(t, "package-lock.json").unwrap();
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|d| d.name == "zod" && d.version == "3.23.8" && d.direct));
        assert!(d.iter().any(|d| d.name == "@types/node" && d.direct));
    }

    #[test]
    fn pnpm_v9_and_v6() {
        let v9 = "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      next:\n        specifier: ^15.0.0\n        version: 15.0.3(react@19.0.0)\n      '@tanstack/react-query':\n        specifier: ^5\n        version: 5.59.0(react@19.0.0)\npackages:\n  next@15.0.3:\n    resolution: {integrity: x}\n  '@swc/helpers@0.5.13':\n    resolution: {integrity: y}\n";
        let d = pnpm_lock(v9).unwrap();
        assert!(d.iter().any(|d| d.name == "next" && d.version == "15.0.3" && d.direct));
        assert!(d.iter().any(|d| d.name == "@tanstack/react-query" && d.version == "5.59.0"));
        assert!(d.iter().any(|d| d.name == "@swc/helpers" && d.version == "0.5.13" && !d.direct));
        let v5 = "lockfileVersion: 5.4\nspecifiers:\n  zod: ^3.0.0\ndependencies:\n  zod: 3.21.4\npackages:\n  /zod/3.21.4:\n    resolution: {integrity: z}\n";
        let d = pnpm_lock(v5).unwrap();
        assert_eq!(d.iter().filter(|d| d.name == "zod").count(), 1);
        assert_eq!(d[0].version, "3.21.4");
    }

    #[test]
    fn bun_and_yarn() {
        let bun = r#"{
  "lockfileVersion": 1,
  "workspaces": { "": { "name": "app", "dependencies": { "zod": "^4.0.0", }, }, },
  "packages": {
    "zod": ["zod@4.1.5", "", {}, "sha512-x"],
    "@types/react": ["@types/react@19.0.1", "", {}, "sha512-y"],
    "a/zod": ["zod@3.0.0", "", {}, "sha512-z"],
  }
}"#;
        let d = bun_lock(bun).unwrap();
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|d| d.name == "zod" && d.version == "4.1.5" && d.direct));
        let y1 = "# yarn lockfile v1\n\n\"@babel/core@^7.0.0\", \"@babel/core@^7.1.0\":\n  version \"7.24.0\"\n  resolved \"x\"\n\nzod@^3.22.0:\n  version \"3.22.4\"\n";
        let direct: HashSet<String> = ["zod".to_string()].into();
        let d = yarn_lock(y1, &direct);
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|d| d.name == "@babel/core" && d.version == "7.24.0"));
        let berry = "__metadata:\n  version: 8\n\n\"zod@npm:^3.22.0\":\n  version: 3.23.8\n  resolution: \"zod@npm:3.23.8\"\n";
        let d = yarn_lock(berry, &direct);
        assert_eq!(d, vec![dep(Eco::Npm, "zod", "3.23.8", true, "yarn.lock")]);
    }

    #[test]
    fn cargo_and_python_and_go() {
        let c = "version = 4\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"tokio\", \"axum 0.8.1\"]\n\n[[package]]\nname = \"tokio\"\nversion = \"1.47.1\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"axum\"\nversion = \"0.8.1\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"bytes\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
        let d = cargo_lock(c).unwrap();
        assert_eq!(d.len(), 3);
        assert!(d.iter().any(|d| d.name == "axum" && d.direct));
        assert!(d.iter().any(|d| d.name == "bytes" && !d.direct));

        let uv = "version = 1\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\nsource = { editable = \".\" }\ndependencies = [{ name = \"pydantic\" }]\n\n[[package]]\nname = \"pydantic\"\nversion = \"2.9.2\"\nsource = { registry = \"https://pypi.org/simple\" }\n\n[[package]]\nname = \"annotated-types\"\nversion = \"0.7.0\"\nsource = { registry = \"https://pypi.org/simple\" }\n";
        let d = uv_lock(uv).unwrap();
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|d| d.name == "pydantic" && d.version == "2.9.2" && d.direct));

        let r = requirements(
            "pydantic==1.10.18  # pinned\nfastapi[all]==0.110.0 ; python_version>'3'\n-r other.txt\nrequests>=2\n",
            "requirements.txt",
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].name, "fastapi");

        let g = go_mod("module x\n\ngo 1.22\n\nrequire github.com/gin-gonic/gin v1.10.0\n\nrequire (\n\tgithub.com/spf13/cobra v1.8.1\n\tgolang.org/x/sys v0.20.0 // indirect\n)\n\nreplace github.com/spf13/cobra => github.com/spf13/cobra v1.8.0\n");
        assert_eq!(g.len(), 3);
        assert!(g.iter().any(|d| d.name == "github.com/spf13/cobra" && d.version == "v1.8.0" && d.direct));
        assert!(g.iter().any(|d| d.name == "golang.org/x/sys" && !d.direct));
    }
}

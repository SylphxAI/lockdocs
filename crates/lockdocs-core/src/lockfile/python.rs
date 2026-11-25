use super::dep;
use crate::{norm_name, Dep, Eco};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub fn is_requirements(name: &str) -> bool {
    name.starts_with("requirements") && name.ends_with(".txt")
}

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
    let pkgs = v.get("package").and_then(|p| p.as_array()).map(Vec::as_slice).unwrap_or_default();
    let mut direct = HashSet::new();
    let mut local = HashSet::new();
    for p in pkgs {
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
    for p in pkgs {
        let (Some(name), Some(ver)) = (p.get("name").and_then(|n| n.as_str()), p.get("version").and_then(|n| n.as_str())) else {
            continue;
        };
        if local.contains(name) {
            continue;
        }
        let from = if p.get("source").is_some_and(|s| s.get("git").is_some()) {
            "uv.lock (git)"
        } else {
            "uv.lock"
        };
        out.push(dep(Eco::PyPI, name, ver, direct.contains(&norm_name(Eco::PyPI, name)), from));
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
        let origin = if p
            .get("source")
            .is_some_and(|s| s.get("git").is_some() || s.get("type").and_then(toml::Value::as_str) == Some("git"))
        {
            format!("{from} (git)")
        } else {
            from.to_string()
        };
        out.push(dep(Eco::PyPI, name, ver, is_direct, &origin));
    }
    Ok(out)
}

pub fn pipfile_lock(text: &str) -> anyhow::Result<Vec<Dep>> {
    let v: Value = serde_json::from_str(text)?;
    let mut out = Vec::new();
    for sect in ["default", "develop"] {
        for (name, p) in v.get(sect).and_then(|s| s.as_object()).into_iter().flatten() {
            if let Some(ver) = p.get("version").and_then(|v| v.as_str()) {
                out.push(dep(
                    Eco::PyPI,
                    name,
                    ver.trim_start_matches("=="),
                    true,
                    if p.get("git").is_some() { "Pipfile.lock (git)" } else { "Pipfile.lock" },
                ));
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

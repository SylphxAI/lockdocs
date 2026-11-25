use super::dep;
use crate::{Dep, Eco};
use std::collections::BTreeMap;

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

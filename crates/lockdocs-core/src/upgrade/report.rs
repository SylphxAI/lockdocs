use super::call_sites::{call_sites, mentions};
use super::diff::{diff, squash};
use super::Report;
use crate::extract::{Entry, Kind};
use crate::index::PackageIndex;
use serde_json::{json, Value};
use std::path::Path;

fn is_guide(file: &str) -> bool {
    let base = file.rsplit('/').next().unwrap_or(file).to_ascii_uppercase();
    ["CHANGELOG", "MIGRAT", "UPGRAD", "BREAKING", "HISTORY", "NEWS"]
        .iter()
        .any(|k| base.contains(k))
}

fn cite(idx: &PackageIndex, e: &Entry) -> String {
    format!("{}@{} {}:{}", idx.name, idx.version, e.file, e.line)
}

/// Migration-guide sections of the target version: those that mention the symbol.
fn guide_for<'a>(idx: &'a PackageIndex, name: &str) -> Vec<&'a Entry> {
    idx.entries
        .iter()
        .filter(|e| e.kind == Kind::Prose && is_guide(&e.file) && !name.is_empty() && mentions(&e.doc, name, false))
        .take(2)
        .collect()
}

fn side(idx: &PackageIndex, e: &Entry) -> Value {
    json!({"cite": cite(idx, e), "kind": e.kind.as_str(), "signature": squash(&e.sig)})
}

pub fn report(root: &Path, old: &PackageIndex, new: &PackageIndex) -> Report {
    let diffs = diff(&old.entries, &new.entries);
    let olds: Vec<&Entry> = diffs.iter().map(|d| &d.old).collect();
    let sites = call_sites(root, old.eco, &old.name, &olds);
    let mut affected = Vec::new();
    let mut text = format!(
        "Upgrade report: {} {} -> {} ({} API changes between the versions)\n",
        old.name,
        old.version,
        new.version,
        diffs.len()
    );
    let mut other = 0;
    for d in &diffs {
        let Some(cs) = sites.get(&d.old.path).filter(|c| !c.is_empty()) else {
            other += 1;
            continue;
        };
        let guide = guide_for(new, &d.old.name);
        text.push_str(&format!(
            "\n{} {}\n  was: {}  [{}]\n",
            d.change.as_str().to_uppercase(),
            d.old.path,
            squash(&d.old.sig),
            cite(old, &d.old)
        ));
        if let Some(n) = &d.new {
            text.push_str(&format!("  now: {}  [{}]\n", squash(&n.sig), cite(new, n)));
        }
        text.push_str(&format!(
            "  call sites: {}\n",
            cs.iter().map(|c| format!("{}:{}", c.path, c.line)).collect::<Vec<_>>().join(", ")
        ));
        for g in &guide {
            text.push_str(&format!("  migration: {} [{}]\n", g.name, cite(new, g)));
        }
        affected.push(json!({
            "change": d.change.as_str(),
            "symbol": d.old.path,
            "old": side(old, &d.old),
            "new": d.new.as_ref().map(|n| side(new, n)),
            "call_sites": cs.iter().map(|c| json!({"path": c.path, "line": c.line})).collect::<Vec<_>>(),
            "migration": guide.iter().map(|g| json!({"section": g.name, "cite": cite(new, g)})).collect::<Vec<_>>(),
        }));
    }
    if affected.is_empty() {
        text.push_str("\nNo changed API is used in this project's source files.\n");
    }
    let guides: Vec<&Entry> = new.entries.iter().filter(|e| e.kind == Kind::Prose && is_guide(&e.file)).take(5).collect();
    if !guides.is_empty() {
        text.push_str("\nMigration guide sections in the target version:\n");
        for g in &guides {
            text.push_str(&format!("  {} [{}]\n", g.name, cite(new, g)));
        }
    }
    text.push_str(&format!(
        "\n{other} other API changes are not used in this project. Call sites are textual matches in files that mention the package.\n"
    ));
    let json = json!({
        "package": old.name, "from": old.version, "to": new.version,
        "api_changes": diffs.len(), "affected": affected, "unaffected_changes": other,
        "migration_guides": guides.iter().map(|g| json!({"section": g.name, "cite": cite(new, g)})).collect::<Vec<_>>(),
    });
    Report { text, json }
}

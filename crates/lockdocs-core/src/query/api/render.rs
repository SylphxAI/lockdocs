//! Token-budgeted text and JSON for exact matches and npm import diagnostics.

use super::matches::{Matches, NpmMiss};
use crate::extract::{Entry, Kind};
use crate::query::entries::resolve_alias;
use crate::query::packages::ReadyPkg;
use crate::query::render::{provenance, render_entry, truncate, Pack};
use crate::query::Answer;
use crate::{est_tokens, Eco};
use serde_json::json;

pub(super) fn npm_miss(ready: &[ReadyPkg], matches: Matches, public_path: &str, requested_import: Option<&str>, reason: NpmMiss, tokens: usize) -> Answer {
    let Matches {
        cands,
        applicability,
        alternatives,
        ..
    } = matches;
    let (status, explanation) = match reason {
        NpmMiss::Ambiguous => (
            "ambiguous",
            "Public declarations disagree across entrypoints or export conditions; no signature was selected.\n",
        ),
        NpmMiss::Unresolved => (
            "unresolved",
            "Import applicability could not be fully resolved from the installed declarations; no unrelated same-named symbol was substituted.\n",
        ),
        NpmMiss::NotExported => (
            "not_exported",
            "This symbol is not exported by the requested import entrypoint. Same-named internal declarations are not a match.\n",
        ),
    };
    let imports: Vec<_> = ready
        .iter()
        .filter(|r| r.0.eco == Eco::Npm)
        .map(|r| requested_import.unwrap_or(&r.0.name))
        .collect();
    let mut out = Pack::new(tokens);
    out.push_raw(&format!(
        "No verified exact public API match for `{public_path}` from `{}` ({}).\n",
        imports.join("`, `"),
        status
    ));
    out.push_block(explanation);
    for a in alternatives.iter().take(8) {
        out.push_block(&format!(
            "Alternate import `{}` ({}): `{}` — {} {}:{}\n",
            a["import"].as_str().unwrap_or(""),
            a["conditions"].as_str().unwrap_or(""),
            a["signature"].as_str().unwrap_or(""),
            a["package"].as_str().unwrap_or(""),
            a["file"].as_str().unwrap_or(""),
            a["line"]
        ));
    }
    if matches!(reason, NpmMiss::Ambiguous) {
        for a in applicability.iter().take(8) {
            out.push_block(&format!(
                "Applicable import `{}` ({}) via {} → {}:{}\n",
                a["import"].as_str().unwrap_or(""),
                a["conditions"].as_str().unwrap_or(""),
                a["entrypoint"].as_str().unwrap_or(""),
                a["file"].as_str().unwrap_or(""),
                a["line"]
            ));
        }
        for (_, pi, ei) in cands.iter().take(8) {
            let idx = &ready[*pi].0;
            out.push_block(&render_entry(idx, &idx.api_entries()[*ei], 500));
        }
    }
    Answer {
        json: json!({"status": status, "symbol": public_path, "imports": imports, "provenance": provenance(ready),
        "applicability": applicability, "alternatives": alternatives, "tokens": est_tokens(&out.text)}),
        text: out.text,
    }
}

pub(super) fn exact(ready: &[ReadyPkg], matches: Matches, public_path: &str, requested_import: Option<&str>, tokens: usize) -> Answer {
    let Matches {
        cands,
        applicability,
        alternatives,
        ..
    } = matches;
    let (_, bpi, bei) = cands[0];
    let (idx, dep, note) = &ready[bpi];
    let best0 = &idx.api_entries()[bei];
    let best = if idx.npm_api.is_some() { best0 } else { resolve_alias(idx, best0) };
    let mut out = Pack::new(tokens);
    out.push_raw(&format!(
        "{} · {} · {}{}\n",
        idx.id(),
        idx.eco,
        idx.source,
        if dep.from == "requested" {
            String::new()
        } else {
            format!(" · pinned in {}", dep.from)
        }
    ));
    if let Some(u) = &idx.upstream {
        out.push_raw(&format!("Upstream: {u}\n"));
    }
    if let Some(n) = note {
        out.push_raw(&format!("Note: {n}\n"));
    }
    if !std::ptr::eq(best, best0) {
        out.push_raw(&format!(
            "`{}` is a re-export of `{}` ({}:{}).\n",
            best0.path, best.path, best0.file, best0.line
        ));
    }
    if idx.npm_api.is_some() {
        out.push_raw(&format!(
            "Public import: `{}` · symbol `{public_path}`\n",
            requested_import.unwrap_or(&idx.name)
        ));
        for a in &applicability {
            out.push_block(&format!(
                "Declaration entrypoint: {} ({})\n",
                a["entrypoint"].as_str().unwrap_or(""),
                a["conditions"].as_str().unwrap_or("")
            ));
        }
    }
    out.push_block(&render_entry(idx, best, 6000));
    // Overloads and declaration merging: same path, other entries.
    let overloads: Vec<&Entry> = idx
        .api_entries()
        .iter()
        .filter(|e| e.path == best.path && e.file == best.file && !std::ptr::eq(*e, best) && e.kind != Kind::Prose && e.kind != Kind::Alias)
        .take(8)
        .collect();
    if !overloads.is_empty() {
        let mut s = String::from("Other declarations:\n");
        for e in overloads {
            s.push_str(&format!("- `{}` — {} {}:{}\n", e.sig, idx.id(), e.file, e.line));
        }
        out.push_block(&s);
    }
    let mut members_json = Vec::new();
    if best.kind.is_container() {
        let sep = if idx.eco == Eco::Cargo { "::" } else { "." };
        let prefix = format!("{}{sep}", best.path);
        let members: Vec<&Entry> = idx
            .api_entries()
            .iter()
            .filter(|e| {
                e.path.starts_with(&prefix) && !e.path[prefix.len()..].contains(sep) && e.kind != Kind::Prose && (idx.npm_api.is_none() || e.file == best.file)
            })
            .collect();
        if !members.is_empty() {
            let mut s = format!("Members of {} ({}):\n", best.path, members.len());
            for m in members.iter().take(80) {
                let doc1 = m.doc.lines().next().unwrap_or("");
                s.push_str(&format!(
                    "- `{}`{}{} — {}:{}\n",
                    m.sig,
                    if doc1.is_empty() { "" } else { " — " },
                    truncate(doc1, 120),
                    m.file,
                    m.line
                ));
                members_json.push(json!({"path": m.path, "sig": m.sig, "file": m.file, "line": m.line}));
            }
            out.push_block(&s);
        }
    }
    let others: Vec<String> = cands
        .iter()
        .skip(1)
        .filter(|(_, pi, ei)| {
            let e = &ready[*pi].0.api_entries()[*ei];
            !(e.path == best.path && e.file == best.file) && !std::ptr::eq(e, best0)
        })
        .take(8)
        .map(|(_, pi, ei)| {
            let (ix, _, _) = &ready[*pi];
            let e = &ix.api_entries()[*ei];
            format!("- {} `{}` — {} {}:{}", e.kind.as_str(), e.path, ix.id(), e.file, e.line)
        })
        .collect();
    if !others.is_empty() {
        out.push_block(&format!("Other matches:\n{}\n", others.join("\n")));
    }
    Answer {
        json: json!({
            "provenance": provenance(ready), "package": idx.id(), "kind": best.kind.as_str(), "path": best.path, "signature": best.sig, "doc": best.doc,
            "file": best.file, "line": best.line, "members": members_json, "tokens": est_tokens(&out.text),
            "status": "exact", "import": if idx.npm_api.is_some() { Some(requested_import.unwrap_or(&idx.name)) } else { None },
            "applicability": applicability, "alternatives": alternatives,
        }),
        text: out.text,
    }
}

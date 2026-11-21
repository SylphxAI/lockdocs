// Keep query::tests paths stable for existing test filters.
use super::packages::fetch_report;
use super::ranking::{deprecated_page, heading_terms, old_upgrade_guide};
use super::render::lead;
use super::{automatic_upstream, redirects, Options, Workspace};
use crate::extract::{Entry, Kind};
use crate::{upstream, Dep, Eco};
use serde_json::json;
use std::sync::Arc;

#[test]
fn fetch_report_preserves_flat_counts_and_adds_full_provenance() {
    let dep = Dep {
        eco: Eco::Cargo,
        name: "axum".into(),
        version: "0.7.9".into(),
        direct: true,
        from: "Cargo.lock".into(),
    };
    let status: upstream::Manifest = serde_json::from_value(json!({
        "format": upstream::FORMAT, "repo": "github.com/tokio-rs/axum", "tag": "axum-v0.7.9",
        "commit": "0123456789abcdef0123456789abcdef01234567", "files": 20, "bytes": 100, "note": null,
    }))
    .unwrap();
    let report = fetch_report(&dep, &status);
    assert_eq!(report["files"], 20);
    assert_eq!(report["files"], report["upstream"]["files"]);
    assert_eq!(report["tag"], report["upstream"]["tag"]);
    assert_eq!(report["repo"], report["upstream"]["repo"]);
    assert!(report.get("note").is_some());
}

#[test]
fn upstream_is_default_with_explicit_opt_out() {
    assert!(automatic_upstream(None, false));
    assert!(automatic_upstream(Some("1"), false));
    for value in ["0", "false", "no"] {
        assert!(!automatic_upstream(Some(value), false));
    }
    assert!(!automatic_upstream(None, true));
}

#[test]
fn workspace_does_not_reuse_network_policy() {
    let ws = Workspace::default();
    let root = std::env::temp_dir();
    let online = Options {
        fetch: false,
        upstream: true,
        private: false,
    };
    let offline = Options {
        fetch: false,
        upstream: false,
        private: false,
    };
    let a = ws.engine(&root, &online);
    let b = ws.engine(&root, &offline);
    assert!(!Arc::ptr_eq(&a, &b));
    assert!(!b.opts.upstream);
}

#[test]
fn old_upgrade_guides_and_deprecated_pages() {
    let e = |name: &str, file: &str| Entry {
        kind: Kind::Prose,
        name: name.into(),
        path: name.into(),
        file: file.into(),
        line: 1,
        sig: String::new(),
        doc: String::new(),
        alias_of: None,
        legacy: false,
        prose_imports: Vec::new(),
        prose_parents: Vec::new(),
    };
    assert!(old_upgrade_guide(
        &e("Migrating to v6.0.0 › x", "upstream:docs/src/use/migrating-to-6.0.0.md"),
        9
    ));
    assert!(old_upgrade_guide(
        &e("Upgrade to Prisma ORM 4 › x", "upstream:docs/700-upgrading-to-prisma-4.mdx"),
        6
    ));
    assert!(!old_upgrade_guide(
        &e("Upgrade to Prisma ORM 6 › x", "upstream:docs/500-upgrading-to-prisma-6.mdx"),
        6
    ));
    assert!(!old_upgrade_guide(&e("Migration Guide › x", "upstream:docs/migration.md"), 2));
    assert!(deprecated_page(&e("Configure Language Options (Deprecated) › Globals", "x.md")));
    assert!(!heading_terms("Seeding with TypeScript or JavaScript").contains(&"type".to_string()));
    assert!(heading_terms("useActionState reference").contains(&"action".to_string()));
    let l = lead("Intro line.\n\n```css\n@import \"tailwindcss\";\n\n@custom-variant dark (&:where(.dark, .dark *));\n```\n\nMore.");
    assert!(l.contains("@custom-variant") && l.ends_with("```"), "{l}");
    let l = lead("Three helpers:\n\n* **`parse_obj`**: from a dict\n  more.\n* `parse_raw`\n\nNext.");
    assert!(l.contains("parse_obj") && l.contains("parse_raw") && !l.contains("Next"), "{l}");
    assert!(lead("<Deprecated>\n\nIn React 19, it is no longer necessary.\n\n</Deprecated>").starts_with("In React 19"));
}

#[test]
fn deprecation_redirects() {
    assert_eq!(redirects("Consider `z.strictObject(A.shape)` instead"), vec!["strictObject"]);
    assert_eq!(
        redirects("The `parse_obj` method is deprecated; use `model_validate` instead."),
        vec!["model_validate"]
    );
    assert_eq!(redirects("@deprecated Use .extend instead"), vec!["extend"]);
    assert_eq!(redirects("You can use this to parse data"), Vec::<String>::new());
    assert_eq!(redirects("Renamed to OptionalFromRequestParts."), vec!["OptionalFromRequestParts"]);
}

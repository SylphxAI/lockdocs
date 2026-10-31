//! Offline, package-independent multi-entrypoint regressions. No real package
//! names or versions are involved in the resolver policy.
use lockdocs_core::query::{Engine, Options};
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}
fn fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "package.json", r#"{"name":"consumer","dependencies":{"@sample/forms":"1.2.3"}}"#);
    write(
        dir.path(),
        "package-lock.json",
        &json!({"name":"consumer","lockfileVersion":3,"packages":{
            "":{"dependencies":{"@sample/forms":"1.2.3"}}, "node_modules/@sample/forms":{"version":"1.2.3"}
        }})
        .to_string(),
    );
    let pkg = dir.path().join("node_modules/@sample/forms");
    write(
        &pkg,
        "package.json",
        r#"{"name":"@sample/forms","version":"1.2.3","exports":{
        ".":{"types":"./index.d.cts","default":"./index.cjs"},
        "./next":{"types":"./next/index.d.ts","default":"./next/index.js"},
        "./conditional":{"import":{"types":"./import.d.mts"},"require":{"types":"./require.d.cts"}},
        "./cycle":"./cycle.d.ts", "./missing":"./missing.d.ts", "./features/*":"./features/*.d.ts", "./features/blocked":null, "./internal-api":"./internal/types.d.ts"
    }}"#,
    );
    write(
        &pkg,
        "index.d.cts",
        "import * as f from './stable/index.cjs';\nexport * from './stable/index.cjs';\nexport { f, f as default };\n",
    );
    write(
        &pkg,
        "stable/index.d.cts",
        "export * from './types.cjs';\nexport { build as create } from './types.cjs';\n",
    );
    write(
        &pkg,
        "stable/types.d.cts",
        "/** Stable builder. */\nexport declare function build(message?: string): Form;\nexport declare class Form { strict(message?: string): Form; }\n",
    );
    write(&pkg, "next/index.d.ts", "export * from './types.js';\n");
    write(&pkg, "next/types.d.ts", "export declare function build(): ModernForm;\nexport declare class Form { strict(): ModernForm; }\nexport declare function email(options?: Options): EmailSchema;\n");
    write(
        &pkg,
        "unexported.d.ts",
        "export declare const email: RegExp;\nexport declare class Form { strict(): WrongForm; }\n",
    );
    write(&pkg, "import.d.mts", "export declare function build(value: string): string;\n");
    write(&pkg, "require.d.cts", "export declare function build(value: number): number;\n");
    write(&pkg, "cycle.d.ts", "export * from './cycle2.js';\nexport * from './stable/types.cjs';\n");
    write(&pkg, "cycle2.d.ts", "export * from './cycle.js';\n");
    write(&pkg, "features/one.d.ts", "export declare function build(one: boolean): boolean;\n");
    write(&pkg, "features/blocked.d.ts", "export declare function build(blocked: never): never;\n");
    write(&pkg, "internal/types.d.ts", "export declare function build(visible: string): string;\n");
    dir
}
fn engine(dir: &Path) -> Engine {
    Engine::new(
        dir,
        Options {
            fetch: false,
            upstream: false,
            private: false,
        },
    )
}
#[test]
fn default_public_entrypoint_beats_same_named_alternate_and_internal_symbols() {
    let dir = fixture();
    let e = engine(dir.path());
    for symbol in ["Form.strict", "f.Form.strict", "default.Form.strict"] {
        let a = e.api(symbol, Some("@sample/forms"), 1200).unwrap();
        assert_eq!(a.json["status"], "exact", "{}", a.text);
        assert_eq!(a.json["signature"], "strict(message?: string): Form");
        assert_eq!(a.json["file"], "stable/types.d.cts");
        assert_eq!(a.json["line"], 3);
        assert_eq!(a.json["import"], "@sample/forms");
    }
    let a = e.api("create", Some("@sample/forms"), 1200).unwrap();
    assert_eq!(a.json["doc"], "Stable builder.");
    assert!(a.json["signature"].as_str().unwrap().contains("build(message?: string)"), "{}", a.text);
    let a = e.api("email", Some("@sample/forms"), 1200).unwrap();
    assert_eq!(a.json["status"], "not_exported", "{}", a.text);
    assert!(a.json.get("signature").is_none());
    assert!(a.text.contains("Alternate import `@sample/forms/next`"));
    assert!(!a.text.contains("RegExp"));
    let a = e.api("NotAForm.strict", Some("@sample/forms"), 1200).unwrap();
    assert_ne!(a.json["status"], "exact");
}
#[test]
fn alternate_imports_and_export_patterns_remain_queryable() {
    let dir = fixture();
    let e = engine(dir.path());
    for (symbol, package) in [("Form.strict", Some("@sample/forms/next")), ("@sample/forms/next.Form.strict", None)] {
        let a = e.api(symbol, package, 1200).unwrap();
        assert_eq!(a.json["status"], "exact", "{}", a.text);
        assert_eq!(a.json["signature"], "strict(): ModernForm");
        assert_eq!(a.json["import"], "@sample/forms/next");
    }
    let a = e.api("build", Some("npm:@sample/forms/features/one@1.2.3"), 1200).unwrap();
    assert_eq!(a.json["status"], "exact", "{}", a.text);
    assert!(a.text.contains("one: boolean"));
}
#[test]
fn conditional_disagreement_and_missing_graph_are_not_exact_matches() {
    let dir = fixture();
    let e = engine(dir.path());
    let a = e.api("build", Some("@sample/forms/conditional"), 1200).unwrap();
    assert_eq!(a.json["status"], "ambiguous", "{}", a.text);
    assert!(a.json.get("signature").is_none());
    assert!(a.json["applicability"].to_string().contains("require/types"));
    let a = e.api("build", Some("@sample/forms/missing"), 1200).unwrap();
    assert_eq!(a.json["status"], "unresolved");
    let a = e.api("build", Some("@sample/forms/cycle"), 1200).unwrap();
    assert_eq!(a.json["status"], "exact", "{}", a.text);
}

#[test]
fn umd_global_namespace_does_not_make_explicit_module_exports_unresolved() {
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    let index = std::fs::read_to_string(pkg.join("index.d.cts")).unwrap();
    write(&pkg, "index.d.cts", &format!("{index}\nexport as namespace Forms;\n"));
    let a = engine(dir.path()).api("f.build", Some("@sample/forms"), 1200).unwrap();
    assert_eq!(a.json["status"], "exact", "{}", a.text);
    assert!(a.text.contains("build(message?: string)"), "{}", a.text);
}
#[test]
fn same_version_declaration_and_export_edits_invalidate_disk_and_live_caches() {
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    let e = engine(dir.path());
    let a = e.api("build", Some("@sample/forms"), 1200).unwrap();
    assert!(a.text.contains("message?: string"));
    write(&pkg, "stable/types.d.cts", "export declare function build(message?: number): Form;\n");
    let a = e.api("build", Some("@sample/forms"), 1200).unwrap();
    assert!(a.text.contains("message?: number"), "{}", a.text);
    let a = engine(dir.path()).api("build", Some("@sample/forms"), 1200).unwrap();
    assert!(a.text.contains("message?: number"), "{}", a.text);
    let text = std::fs::read_to_string(pkg.join("package.json"))
        .unwrap()
        .replace("./index.d.cts", "./next/index.d.ts");
    write(&pkg, "package.json", &text);
    let a = e.api("build", Some("@sample/forms"), 1200).unwrap();
    assert!(a.text.contains("build(): ModernForm"), "{}", a.text);
    let a = engine(dir.path()).api("build", Some("@sample/forms"), 1200).unwrap();
    assert!(a.text.contains("build(): ModernForm"), "{}", a.text);
}

#[test]
fn public_edges_not_directory_or_name_heuristics_decide_visibility() {
    let dir = fixture();
    let e = engine(dir.path());
    let a = e.api("build", Some("@sample/forms/internal-api"), 1200).unwrap();
    assert_eq!(a.json["status"], "exact", "{}", a.text);
    assert_eq!(a.json["file"], "internal/types.d.ts");
    let a = e.api("build", Some("@sample/forms/features/blocked"), 1200).unwrap();
    assert_ne!(a.json["status"], "exact");
}

#[test]
fn separately_installed_types_keep_public_namespace_and_real_citation() {
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    std::fs::remove_dir_all(&pkg).unwrap();
    write(&pkg, "package.json", r#"{"name":"@sample/forms","version":"1.2.3","main":"index.js"}"#);
    write(&pkg, "index.js", "module.exports = {};\n");
    let types = dir.path().join("node_modules/@types/sample__forms");
    write(
        &types,
        "package.json",
        r#"{"name":"@types/sample__forms","version":"1.0.0","types":"index.d.ts"}"#,
    );
    write(
        &types,
        "index.d.ts",
        "export = Forms;\ndeclare namespace Forms { function build(input: string): string; }\n",
    );
    let a = engine(dir.path()).api("build", Some("@sample/forms"), 1200).unwrap();
    assert_eq!(a.json["status"], "exact", "{}", a.text);
    assert_eq!(a.json["file"], "@types/sample__forms/index.d.ts");
    assert!(a.text.contains("build(input: string)"), "{}", a.text);
}

#[test]
fn npm_doc_content_changes_also_invalidate_the_live_and_disk_index() {
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    write(&pkg, "README.md", "# Cache example\n\nThe documentation generation is first.\n");
    let e = engine(dir.path());
    let a = e.docs(Some("@sample/forms"), "cache example", 1200).unwrap();
    assert!(a.text.contains("generation is first"), "{}", a.text);
    write(&pkg, "README.md", "# Cache example\n\nThe documentation generation is later.\n");
    for a in [
        e.docs(Some("@sample/forms"), "cache example", 1200).unwrap(),
        engine(dir.path()).docs(Some("@sample/forms"), "cache example", 1200).unwrap(),
    ] {
        assert!(a.text.contains("generation is later"), "{}", a.text);
        assert!(!a.text.contains("generation is first"), "{}", a.text);
    }
}

#[test]
fn mirrored_declaration_files_do_not_change_the_selected_documentation_corpus() {
    use lockdocs_core::{index, locate::Source, Dep, Eco};
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    // Alternate identities stay available to API resolution, without
    // multiplying the root import's BM25/embedding candidate population.
    let src = Source {
        dir: pkg.clone(),
        version: "1.2.3".into(),
        label: "local".into(),
        fetched: false,
        files: None,
        metadata: None,
    };
    let dep = Dep {
        eco: Eco::Npm,
        name: "@sample/forms".into(),
        version: "1.2.3".into(),
        direct: true,
        from: "package-lock.json".into(),
    };
    let before = index::build(&dep, &src, dir.path(), None);
    write(&pkg, "next/types.d.cts", &std::fs::read_to_string(pkg.join("next/types.d.ts")).unwrap());
    let after = index::build(&dep, &src, dir.path(), None);
    let selected = |idx: &index::PackageIndex| {
        idx.docs_indices(None)
            .iter()
            .map(|&i| serde_json::to_value(&idx.entries[i]).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(selected(&before), selected(&after));
    assert!(after.npm_api.as_ref().unwrap().entries.len() > before.npm_api.as_ref().unwrap().entries.len());
}

#[test]
fn docs_and_overview_search_only_the_requested_import_graph() {
    let dir = fixture();
    let e = engine(dir.path());
    for package in [Some("@sample/forms"), Some("npm:@sample/forms@1.2.3"), None] {
        let a = e.docs(package, "build strict email", 4000).unwrap();
        let hits = a.json["hits"].as_array().unwrap();
        assert!(!hits.is_empty(), "{}", a.text);
        assert!(hits.iter().all(|h| h["file"].as_str().unwrap().starts_with("stable/")), "{}", a.text);
        assert!(a.text.contains("message?: string"), "{}", a.text);
        assert!(
            !a.text.contains("ModernForm") && !a.text.contains("WrongForm") && !a.text.contains("RegExp"),
            "{}",
            a.text
        );
    }
    let a = e.docs(Some("@sample/forms"), "", 4000).unwrap();
    assert!(!a.text.contains("email"), "{}", a.text);
    for package in ["@sample/forms/next", "npm:@sample/forms/next@1.2.3"] {
        let a = e.docs(Some(package), "build strict email", 4000).unwrap();
        assert!(a.text.contains("ModernForm") && a.text.contains("EmailSchema"), "{}", a.text);
        assert!(a.json["hits"].as_array().unwrap().iter().all(|h| h["file"] == "next/types.d.ts"), "{}", a.text);
    }
    assert!(e.docs(Some("@sample/forms/features/blocked"), "build", 4000).is_err());
    assert!(e.docs(Some("@sample/forms/not-exported"), "build", 4000).is_err());
}

#[test]
fn legacy_packages_keep_unrestricted_subpath_docs_and_explicit_imports() {
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    write(&pkg, "package.json", r#"{"name":"@sample/forms","version":"1.2.3","types":"index.d.cts"}"#);
    write(&pkg, "headers.d.ts", "export * from './request/headers.js';\n");
    write(
        &pkg,
        "request/headers.d.ts",
        "/** Read incoming request headers. */\nexport declare function headers(): ReadonlyHeaders;\n",
    );
    let e = engine(dir.path());
    let a = e.docs(Some("@sample/forms"), "incoming request headers", 4000).unwrap();
    assert!(a.text.contains("headers(): ReadonlyHeaders"), "{}", a.text);
    for import in ["@sample/forms/headers", "@sample/forms/request/headers"] {
        let a = e.docs(Some(import), "incoming request headers", 4000).unwrap();
        assert!(a.text.contains("headers(): ReadonlyHeaders"), "{}", a.text);
        assert!(
            a.json["hits"].as_array().unwrap().iter().all(|h| h["file"] == "request/headers.d.ts"),
            "{}",
            a.text
        );
        let a = e.api("headers", Some(import), 1200).unwrap();
        assert_eq!(a.json["status"], "exact", "{}", a.text);
        assert_eq!(a.json["import"], import);
    }
    let a = e.api("headers", Some("@sample/forms"), 1200).unwrap();
    assert_eq!(a.json["status"], "not_exported", "{}", a.text);
    assert!(e.docs(Some("@sample/forms/absent"), "headers", 1200).is_err());
}

#[test]
fn docs_aliases_and_deprecation_redirects_cannot_escape_the_import_graph() {
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    write(
        &pkg,
        "stable/types.d.cts",
        "/** Use `email` instead to build strict forms. */\nexport declare function build(message?: string): StableForm;\n",
    );
    let a = engine(dir.path()).docs(Some("@sample/forms"), "create build", 4000).unwrap();
    assert!(a.text.contains("StableForm"), "{}", a.text);
    assert!(
        !a.text.contains("ModernForm") && !a.text.contains("RegExp") && !a.text.contains("EmailSchema"),
        "{}",
        a.text
    );
    assert!(
        a.json["hits"].as_array().unwrap().iter().all(|h| h["file"] == "stable/types.d.cts"),
        "{}",
        a.text
    );
}

#[test]
fn docs_keep_the_selected_identity_when_editions_ship_identical_declarations() {
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    let shared = "/** Shared behavior. */\nexport declare function shared(): string;\n";
    write(&pkg, "stable/same.d.cts", shared);
    write(&pkg, "next/same.d.ts", shared);
    write(&pkg, "stable/index.d.cts", "export * from './same.cjs';\n");
    write(&pkg, "next/index.d.ts", "export * from './same.js';\n");
    let e = engine(dir.path());
    for (package, file) in [("@sample/forms", "stable/same.d.cts"), ("@sample/forms/next", "next/same.d.ts")] {
        let a = e.docs(Some(package), "shared behavior", 1200).unwrap();
        assert_eq!(a.json["hits"].as_array().unwrap().len(), 1, "{}", a.text);
        assert_eq!(a.json["hits"][0]["file"], file, "{}", a.text);
    }
}

#[test]
fn docs_refresh_export_selection_and_refuse_a_different_installed_version() {
    let dir = fixture();
    let pkg = dir.path().join("node_modules/@sample/forms");
    let e = engine(dir.path());
    assert!(!e.docs(Some("@sample/forms"), "build", 4000).unwrap().text.contains("ModernForm"));
    let metadata = std::fs::read_to_string(pkg.join("package.json")).unwrap();
    write(&pkg, "package.json", &metadata.replace("./index.d.cts", "./next/index.d.ts"));
    for e in [&e, &engine(dir.path())] {
        let a = e.docs(Some("@sample/forms"), "build strict", 4000).unwrap();
        assert!(a.text.contains("ModernForm") && !a.text.contains("message?: string"), "{}", a.text);
    }
    write(
        &pkg,
        "package.json",
        &metadata.replace("1.2.3", "2.0.0").replace("./index.d.cts", "./next/index.d.ts"),
    );
    let error = e.docs(Some("@sample/forms"), "build", 1200).err().unwrap();
    assert!(error.contains("refusing to substitute"), "{error}");
    let lock = std::fs::read_to_string(dir.path().join("package-lock.json")).unwrap();
    write(dir.path(), "package-lock.json", &lock.replace("1.2.3", "2.0.0"));
    let e = engine(dir.path());
    let a = e.docs(Some("@sample/forms"), "build strict", 4000).unwrap();
    assert_eq!(a.json["packages"][0], "@sample/forms@2.0.0");
    assert!(a.text.contains("ModernForm") && !a.text.contains("message?: string"), "{}", a.text);
    assert!(e.docs(Some("npm:@sample/forms@1.2.3"), "build", 1200).is_err());
}

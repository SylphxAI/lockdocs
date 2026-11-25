use super::Lockdocs;
use crate::pro;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use lockdocs_core::query::{Options, Workspace};
use mcp_kit::licence::LicencePolicy;
use mcp_kit::server::{App, Call};
use serde_json::{json, Value};
use std::path::Path;

mod private_sources;

const TEST_ENV: &str = "LOCKDOCS_TEST_LICENCE_TOKEN";

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn token(key: &SigningKey, payload: &str) -> String {
    let sig = key.sign(payload.as_bytes());
    format!("{}.{}", URL_SAFE_NO_PAD.encode(payload), URL_SAFE_NO_PAD.encode(sig.to_bytes()))
}

/// A project pinning tiny-schema 1.0.0 (installed) whose target 2.0.0 sits in the fetch cache.
fn project(dir: &Path, cache: &Path) {
    write(&dir.join("package.json"), r#"{"name":"app","dependencies":{"tiny-schema":"^1.0.0"}}"#);
    write(
        &dir.join("package-lock.json"),
        r#"{"name":"app","lockfileVersion":3,"packages":{"":{"name":"app","dependencies":{"tiny-schema":"^1.0.0"}},"node_modules/tiny-schema":{"version":"1.0.0"}}}"#,
    );
    let v1 = dir.join("node_modules/tiny-schema");
    write(&v1.join("package.json"), r#"{"name":"tiny-schema","version":"1.0.0","types":"index.d.ts"}"#);
    write(
        &v1.join("index.d.ts"),
        "export declare namespace s {\n  /** Creates an object schema. */\n  function object<T>(shape: T): ObjectSchema<T>;\n  /** Creates a string schema. */\n  function string(): Schema<string>;\n  /** Old helper. */\n  function legacy(): void;\n  /** Unused helper. */\n  function unused(): void;\n}\n",
    );
    write(
        &dir.join("src/app.ts"),
        "import { s } from \"tiny-schema\";\nconst a = s.object({});\nconst b = s.string();\ns.legacy();\n",
    );
    let dep = lockdocs_core::Dep {
        eco: lockdocs_core::Eco::Npm,
        name: "tiny-schema".into(),
        version: "2.0.0".into(),
        direct: false,
        from: "requested".into(),
    };
    let v2 = lockdocs_core::fetch::fetched_dir(&dep);
    assert!(v2.starts_with(cache));
    write(&v2.join("package.json"), r#"{"name":"tiny-schema","version":"2.0.0","types":"index.d.ts"}"#);
    write(
        &v2.join("index.d.ts"),
        "export declare namespace s {\n  /** Creates an object schema. */\n  function object<T>(shape: T, strict: boolean): ObjectSchema<T>;\n  /** Creates a string schema. */\n  function text(): Schema<string>;\n}\n",
    );
    write(
        &v2.join("MIGRATION.md"),
        "# Migrating to 2\n\n## Removed helpers\n\n`legacy` is gone; delete the call.\n",
    );
    write(&v2.join(".lockdocs-complete"), "");
}

fn app(policy: LicencePolicy<'static>) -> Lockdocs {
    Lockdocs {
        ws: Workspace::default(),
        opts: Options {
            fetch: false,
            upstream: false,
            private: false,
        },
        default_root: None,
        policy,
    }
}

fn call(app: &Lockdocs, name: &str, args: Value) -> Value {
    let call = Call { client_roots: vec![] };
    serde_json::to_value(app.call_result(name, &args, &call)).unwrap()
}

fn text(v: &Value) -> String {
    v["content"][0]["text"].as_str().unwrap_or_default().to_string()
}

// One test owns the process-global env (cache dir, token) so it cannot race.
#[test]
fn upgrade_report_is_gated_and_free_paths_are_not() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("cache");
    std::env::set_var("LOCKDOCS_CACHE", &cache);
    std::env::remove_var(TEST_ENV);
    let proj = tmp.path().join("proj");
    project(&proj, &cache);
    let root = proj.to_str().unwrap();

    let key = SigningKey::from_bytes(&[7; 32]);
    let public = URL_SAFE_NO_PAD.encode(key.verifying_key().to_bytes());
    let keys: &'static [&'static str] = Box::leak(vec![Box::leak(public.into_boxed_str()) as &'static str].into_boxed_slice());
    let policy = LicencePolicy {
        product: "lockdocs",
        require_product: true,
        accepted_plans: &["pro"],
        public_keys: keys,
        env_var: TEST_ENV,
        file_name: "licence-test-never-exists",
        upgrade_url: "https://example.com/pro",
        checkout_base: None,
        tier: "Pro",
        // Money checkout is injected only into the CLI, never the MCP tools.
    };
    let a = app(policy);
    let upgrade = json!({"query": "upgrade", "package": "tiny-schema", "upgrade_to": "2.0.0", "root": root});

    // Unlicensed: a normal result carrying pro_required, never an error.
    let r = call(&a, "docs", upgrade.clone());
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["pro_required"]["product"], "lockdocs");
    assert_eq!(r["structuredContent"]["pro_required"]["feature"], pro::UPGRADE_REPORT);
    assert!(!text(&r).contains("RENAMED"));
    // ...whose text is a local preview: counts and a sample, not the list.
    let pv = text(&r);
    assert!(
        pv.contains("tiny-schema 1.0.0 -> 2.0.0: 3 of the APIs your project calls change (1 removed, 1 renamed, 1 re-signed) across 3 call sites"),
        "{pv}"
    );
    assert!(pv.contains("current per-seat pricing") && pv.contains("https://example.com/pro"), "{pv}");
    assert!(!pv.contains("was:") && !pv.contains("now:"), "{pv}");
    // JSON form keeps the marker and the counts.
    let pj = call(
        &a,
        "docs",
        json!({"query": "x", "package": "tiny-schema", "upgrade_to": "2.0.0", "root": root, "format": "json"}),
    );
    assert!(pj["structuredContent"]["pro_required"].is_object(), "{pj}");
    let pj: Value = serde_json::from_str(&text(&pj)).unwrap();
    assert_eq!(pj["call_sites"], 3);
    // Preview needs the same target files as the report; without them the answer is the plain Pro one.
    let r2 = call(&a, "docs", json!({"query": "x", "package": "nope", "upgrade_to": "2.0.0", "root": root}));
    assert!(
        r2["structuredContent"]["pro_required"].is_object() && text(&r2).contains("Preview unavailable"),
        "{r2}"
    );

    // Tokens that must not unlock: wrong product, wrong plan, expired, signed by another key, junk.
    let other = SigningKey::from_bytes(&[8; 32]);
    for t in [
        token(&key, r#"{"plan":"pro","issuedAt":1,"product":"other-product"}"#),
        token(&key, r#"{"plan":"pro","issuedAt":1}"#),
        token(&key, r#"{"plan":"team","issuedAt":1,"product":"lockdocs"}"#),
        token(&key, r#"{"plan":"pro","issuedAt":1,"product":"lockdocs","expiresAt":1000}"#),
        token(&other, r#"{"plan":"pro","issuedAt":1,"product":"lockdocs"}"#),
        "junk".to_string(),
    ] {
        std::env::set_var(TEST_ENV, &t);
        let r = call(&a, "docs", upgrade.clone());
        assert!(r["structuredContent"]["pro_required"].is_object(), "token {t} unlocked: {r}");
        assert_eq!(r["isError"], false);
    }

    // Free paths never gate, licensed or not.
    std::env::remove_var(TEST_ENV);
    let free = call(&a, "docs", json!({"query": "object schema", "package": "tiny-schema", "root": root}));
    assert!(free["structuredContent"].is_null(), "{free}");
    assert_eq!(free["isError"], false);
    assert!(text(&free).contains("tiny-schema@1.0.0"), "{}", text(&free));
    let resolve = call(&a, "resolve", json!({"root": root}));
    assert!(text(&resolve).contains("tiny-schema@1.0.0"));
    let api = call(&a, "api", json!({"symbol": "s.object", "package": "tiny-schema", "root": root}));
    assert!(text(&api).contains("function object<T>"), "{}", text(&api));
    // Reading another version's docs stays free (an explicit version in `package`).
    let other_version = call(&a, "docs", json!({"query": "migrating", "package": "tiny-schema@2.0.0", "root": root}));
    assert!(other_version["structuredContent"].is_null(), "{other_version}");
    // An empty upgrade_to is not an upgrade request.
    let empty = call(
        &a,
        "docs",
        json!({"query": "object schema", "package": "tiny-schema", "upgrade_to": "", "root": root}),
    );
    assert!(empty["structuredContent"].is_null());

    // Licensed (test key): the report.
    std::env::set_var(TEST_ENV, token(&key, r#"{"plan":"pro","issuedAt":1,"product":"lockdocs","seats":1}"#));
    let r = call(&a, "docs", upgrade.clone());
    assert_eq!(r["isError"], false, "{r}");
    assert!(r["structuredContent"].is_null(), "{r}");
    let t = text(&r);
    assert!(t.contains("SIGNATURE_CHANGED s.object"), "{t}");
    assert!(t.contains("RENAMED s.string"), "{t}");
    assert!(t.contains("now: function text()"), "{t}");
    assert!(t.contains("REMOVED s.legacy"), "{t}");
    assert!(t.contains("src/app.ts:2") && t.contains("src/app.ts:3") && t.contains("src/app.ts:4"), "{t}");
    assert!(
        t.contains("tiny-schema@1.0.0 index.d.ts:") && t.contains("tiny-schema@2.0.0 index.d.ts:"),
        "{t}"
    );
    assert!(t.contains("migration: Migrating to 2 > Removed helpers") || t.contains("MIGRATION.md"), "{t}");
    assert!(!t.contains("s.unused"), "unused symbols are not listed: {t}");
    assert!(t.contains("1 other API changes") || t.contains("other API changes"), "{t}");

    // JSON form.
    let j = call(
        &a,
        "docs",
        json!({"query": "x", "package": "tiny-schema", "upgrade_to": "2.0.0", "root": root, "format": "json"}),
    );
    let parsed: Value = serde_json::from_str(&text(&j)).unwrap();
    assert_eq!(parsed["from"], "1.0.0");
    assert_eq!(parsed["to"], "2.0.0");
    assert_eq!(parsed["affected"].as_array().unwrap().len(), 3, "{parsed}");

    // Errors stay errors once licensed: missing package argument, same version, unknown package.
    let e = call(&a, "docs", json!({"query": "x", "upgrade_to": "2.0.0", "root": root}));
    assert_eq!(e["isError"], true);
    let e = call(&a, "docs", json!({"query": "x", "package": "tiny-schema", "upgrade_to": "1.0.0", "root": root}));
    assert_eq!(e["isError"], true);
    let e = call(&a, "docs", json!({"query": "x", "package": "nope", "upgrade_to": "2.0.0", "root": root}));
    assert_eq!(e["isError"], true);
    std::env::remove_var(TEST_ENV);
    private_sources::check(tmp.path(), a.policy, &key);
}

#[test]
fn real_policy_unlocks_nothing_until_the_key_is_issued() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let t = token(&key, r#"{"plan":"pro","issuedAt":1,"product":"lockdocs"}"#);
    assert!(pro::POLICY.verify(&t).is_err());
    let a = app(pro::POLICY);
    let tools = a.tools();
    let docs = tools.iter().find(|t| t["name"] == "docs").unwrap();
    assert!(docs["inputSchema"]["properties"]["upgrade_to"].is_object());
    assert_eq!(tools.len(), 3, "three tools only");
}

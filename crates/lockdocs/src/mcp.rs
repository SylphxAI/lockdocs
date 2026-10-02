//! The MCP server: lockdocs' tools on mcp-kit (rmcp over stdio).

use crate::{pro, tools};
use lockdocs_core::query::{Options, Workspace};
use mcp_kit::licence::{self, LicencePolicy};
use mcp_kit::rmcp::model::{CallToolResult, ContentBlock};
use mcp_kit::roots::{self, Sources};
use mcp_kit::server::{run_stdio, App, Call, Info};
use serde_json::Value;
use std::path::PathBuf;

const INSTRUCTIONS: &str = "lockdocs answers from the exact dependency versions pinned in this project's lockfiles, using the installed packages' own docs and type declarations, with anonymous release-tag docs fetched on first use unless offline. Cached docs work offline. Call `resolve` to see versions, `docs` with a question (and optionally `package`) before using an API you are unsure of in this version, and `api` for the exact signature of a symbol like `z.object` or `tokio::spawn`. Every section cites package@version path:line.";

struct Lockdocs {
    ws: Workspace,
    opts: Options,
    default_root: Option<PathBuf>,
    policy: LicencePolicy<'static>,
}

impl Lockdocs {
    fn root(&self, args: &Value, call: &Call) -> Result<PathBuf, String> {
        let explicit = ["root", "repo_root"]
            .iter()
            .find_map(|k| args.get(*k).and_then(|v| v.as_str()))
            .map(PathBuf::from);
        roots::pick(&Sources {
            explicit,
            env: &["LOCKDOCS_ROOT"],
            default: self.default_root.clone(),
            client: &call.client_roots,
        })
    }
}

impl App for Lockdocs {
    fn info(&self) -> Info {
        Info {
            name: "lockdocs".into(),
            title: "lockdocs".into(),
            version: crate::VERSION.into(),
            website: "https://sylphxai.github.io/lockdocs/".into(),
            instructions: INSTRUCTIONS.into(),
        }
    }

    fn tools(&self) -> Vec<Value> {
        tools::definitions().as_array().cloned().unwrap_or_default()
    }

    fn call(&self, name: &str, args: &Value, call: &Call) -> Result<String, String> {
        let out = tools::call(&self.ws, &self.opts, name, args, &self.root(args, call)?)?;
        if args.get("format").and_then(|v| v.as_str()) == Some("json") {
            Ok(serde_json::to_string_pretty(&out.json).unwrap_or_default())
        } else {
            Ok(out.text)
        }
    }

    /// Free calls go straight through `call`. The Pro upgrade report (`docs` with
    /// `upgrade_to`) needs a licence; without one the answer is a normal result
    /// with `structuredContent.pro_required`, never an error.
    fn call_result(&self, name: &str, args: &Value, call: &Call) -> CallToolResult {
        let mut note = None;
        if tools::is_upgrade(name, args) {
            match pro::require(&self.policy, pro::UPGRADE_REPORT) {
                Ok(licence) => note = pro::renewal_note(&self.policy, &licence),
                Err(required) => return licence::required_result(&required),
            }
        }
        match self.call(name, args, call) {
            Ok(mut text) => {
                if let Some(n) = note {
                    text.push_str("\n\n");
                    text.push_str(&n);
                }
                CallToolResult::success(vec![ContentBlock::text(text)])
            }
            Err(text) => CallToolResult::error(vec![ContentBlock::text(text)]),
        }
    }

    /// Read the lockfiles as soon as the client connects, so the first call is fast.
    fn warm(&self, call: &Call) {
        if let Ok(root) = self.root(&Value::Null, call) {
            let _ = self.ws.engine(&root, &self.opts);
        }
    }
}

pub fn serve(default_root: Option<PathBuf>, opts: Options) -> anyhow::Result<()> {
    run_stdio(Lockdocs {
        ws: Workspace::default(),
        opts,
        default_root,
        policy: pro::POLICY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    use std::path::Path;

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
            opts: Options { fetch: false, upstream: false },
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
        };
        let a = app(policy);
        let upgrade = json!({"query": "upgrade", "package": "tiny-schema", "upgrade_to": "2.0.0", "root": root});

        // Unlicensed: a normal result carrying pro_required, never an error.
        let r = call(&a, "docs", upgrade.clone());
        assert_eq!(r["isError"], false, "{r}");
        assert_eq!(r["structuredContent"]["pro_required"]["product"], "lockdocs");
        assert_eq!(r["structuredContent"]["pro_required"]["feature"], pro::UPGRADE_REPORT);
        assert!(!text(&r).contains("RENAMED"));

        // Tokens that must not unlock: wrong product, wrong plan, expired, signed by another key, junk.
        let other = SigningKey::from_bytes(&[8; 32]);
        for t in [
            token(&key, r#"{"plan":"pro","issuedAt":1,"product":"repomap"}"#),
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
}

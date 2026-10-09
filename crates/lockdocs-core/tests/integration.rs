//! End-to-end over a fixture project with one dependency per ecosystem,
//! installed in place (node_modules, .venv, vendor/). No network.

use lockdocs_core::query::{Engine, Options};
use std::path::PathBuf;
use std::sync::Once;

fn engine() -> Engine {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let cache = std::env::temp_dir().join(format!("lockdocs-test-cache-{}", std::process::id()));
        std::env::set_var("LOCKDOCS_CACHE", cache);
        std::env::set_var("LOCKDOCS_NO_SYSTEM_PYTHON", "1");
    });
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixture");
    Engine::new(
        &root,
        Options {
            fetch: false,
            upstream: false,
            private: false,
        },
    )
}

#[test]
fn resolves_every_ecosystem() {
    let e = engine();
    let a = e.resolve(None);
    for want in [
        "tiny-schema@1.2.0",
        "tinymodel@2.0.1",
        "tinyrt@0.3.0",
        "example.com/tinyweb@v1.4.0",
        "missing-pkg@2.0.0",
    ] {
        assert!(a.text.contains(want), "missing {want} in\n{}", a.text);
    }
    assert!(a.text.contains("not installed"), "{}", a.text);
    let lockfiles = a.json["lockfiles"].as_array().unwrap().len();
    assert_eq!(lockfiles, 4, "{}", a.text);
}

#[test]
fn docs_cite_package_version_and_line() {
    let e = engine();
    let a = e.docs(Some("tiny-schema"), "reject unknown keys", 1500).unwrap();
    assert!(a.text.contains("Strict mode"), "{}", a.text);
    assert!(a.text.contains("tiny-schema@1.2.0 README.md:9"), "{}", a.text);
    assert!(a.text.contains("tiny-schema@1.2.0 index.d.ts:"), "{}", a.text);
    // Python README from METADATA
    let a = e.docs(Some("tinymodel"), "turn a model into a dict", 1500).unwrap();
    assert!(a.text.contains("to_dict"), "{}", a.text);
}

#[test]
fn api_finds_symbols_in_each_language() {
    let e = engine();
    let a = e.api("tiny-schema.s.object", None, 1000).unwrap();
    assert!(a.text.contains("function object<T>(shape: T): ObjectSchema<T>"), "{}", a.text);
    assert!(a.text.contains("Creates an object schema"), "{}", a.text);
    let a = e.api("ObjectSchema", Some("tiny-schema"), 1000).unwrap();
    assert!(a.text.contains("Members of ObjectSchema") && a.text.contains("strict()"), "{}", a.text);
    let a = e.api("tinyrt::spawn", None, 1000).unwrap();
    assert!(a.text.contains("Spawns a future onto the runtime"), "{}", a.text);
    assert!(a.text.contains("tinyrt@0.3.0 src/task.rs:3"), "{}", a.text);
    let a = e.api("Model.to_dict", Some("tinymodel"), 1000).unwrap();
    assert!(a.text.contains("def to_dict(self, *, exclude_none: bool = False) -> dict"), "{}", a.text);
    let a = e.api("tinyweb.Context.JSON", None, 1000).unwrap();
    assert!(a.text.contains("JSON writes obj as JSON"), "{}", a.text);
}

#[test]
fn missing_packages_explain_themselves() {
    let e = engine();
    let err = e.docs(Some("missing-pkg"), "anything", 1000).err().unwrap();
    assert!(err.contains("not on this machine") && err.contains("--fetch"), "{err}");
    let err = e.docs(Some("nope-not-here"), "x", 1000).err().unwrap();
    assert!(err.contains("not a dependency"), "{err}");
    let err = e.docs(Some("npm:tiny-schema@9.9.9"), "x", 1000).err().unwrap();
    assert!(err.contains("enable fetching"), "{err}");
}

#[test]
fn budget_is_respected() {
    let e = engine();
    let a = e.docs(None, "schema object strict model dict spawn json", 300).unwrap();
    assert!(
        lockdocs_core::est_tokens(&a.text) <= 330,
        "{} tokens:\n{}",
        lockdocs_core::est_tokens(&a.text),
        a.text
    );
}

#[test]
fn first_use_reports_unavailable_upstream_and_reuses_note() {
    let e = engine();
    let root = e.root().to_path_buf();
    let e = Engine::new(
        &root,
        Options {
            fetch: false,
            upstream: true,
            private: false,
        },
    );
    for _ in 0..2 {
        let a = e.docs(Some("tiny-schema"), "reject unknown keys", 1500).unwrap();
        assert!(a.text.contains("upstream docs fetch failed"), "{}", a.text);
        assert_eq!(a.json["provenance"][0]["requested_version"], "1.2.0");
        assert!(a.json["provenance"][0]["note"].as_str().unwrap().contains("no GitHub repository"));
    }
    let broad = e.docs(None, "reject unknown keys", 1200).unwrap();
    assert!(
        broad.text.contains("fallback notes") && broad.text.contains("Fallback tiny-schema@1.2.0"),
        "{}",
        broad.text
    );
    assert!(lockdocs_core::est_tokens(&broad.text) <= 1230, "{}", broad.text);
    for hit in broad.json["hits"].as_array().unwrap() {
        assert!(hit.get("provenance").is_none());
    }
    let offline = engine().docs(Some("tiny-schema"), "reject unknown keys", 1500).unwrap();
    assert!(offline.json["provenance"][0]["note"].is_null());
}

#[test]
fn pinned_upstream_cache_is_reused_online_and_offline_with_provenance() {
    let _ = engine(); // initialize the isolated test cache
    let root = std::env::temp_dir().join(format!("lockdocs-cached-provenance-{}", std::process::id()));
    let dep = lockdocs_core::Dep {
        eco: lockdocs_core::Eco::Npm,
        name: "cached-provenance-fixture".into(),
        version: "1.2.3".into(),
        direct: true,
        from: "package-lock.json".into(),
    };
    let src = root.join("node_modules").join(&dep.name);
    std::fs::create_dir_all(&src).unwrap();
    // No repository metadata: any mistaken refresh would fail instead of contacting GitHub.
    std::fs::write(src.join("package.json"), r#"{"name":"cached-provenance-fixture","version":"1.2.3"}"#).unwrap();
    let cache = lockdocs_core::upstream::dir(&dep);
    std::fs::create_dir_all(cache.join("docs")).unwrap();
    std::fs::write(cache.join("docs/guide.md"), "# Pinned guide\n\nUse release_only_api to reject unknown keys.\n").unwrap();
    let manifest = serde_json::json!({
        "format": lockdocs_core::upstream::FORMAT,
        "repo": "github.com/example/pinned", "tag": "v1.2.3",
        "commit": "0123456789abcdef0123456789abcdef01234567",
        "files": 1, "bytes": 73, "note": null, "site": "github.com/example/site@fixed (major docs)", "pages": [], "docs_sites_checked": true,
    });
    std::fs::write(cache.join(".lockdocs-upstream.json"), manifest.to_string()).unwrap();
    for opts in [
        Options {
            fetch: true,
            upstream: true,
            private: false,
        },
        Options {
            fetch: false,
            upstream: true,
            private: false,
        },
        Options {
            fetch: false,
            upstream: false,
            private: false,
        },
    ] {
        let mut e = Engine::new(&root, opts);
        e.project.deps = vec![dep.clone()];
        let a = e.docs(Some(&dep.name), "reject unknown keys", 1500).unwrap();
        assert!(a.text.contains("release_only_api") && a.text.contains("upstream:docs/guide.md"), "{}", a.text);
        assert_eq!(a.json["provenance"][0]["upstream"]["commit"], manifest["commit"]);
        assert_eq!(a.json["provenance"][0]["upstream"]["site"], manifest["site"]);
        assert_eq!(a.json["provenance"][0]["requested_version"], "1.2.3");
    }
    // A format-3 commit candidate was resolved through /commits/{candidate},
    // which could have been a branch. Online use cannot silently trust it.
    let mut legacy = manifest.clone();
    legacy["format"] = serde_json::json!(3);
    std::fs::write(cache.join(".lockdocs-upstream.json"), legacy.to_string()).unwrap();
    let mut online = Engine::new(
        &root,
        Options {
            fetch: false,
            upstream: true,
            private: false,
        },
    );
    online.project.deps = vec![dep.clone()];
    let rejected = online.docs(Some(&dep.name), "reject unknown keys", 1500).unwrap();
    assert!(rejected.json["provenance"][0]["upstream"].is_null());
    assert!(rejected.text.contains("upstream docs fetch failed"));
    assert!(!rejected.text.contains("release_only_api"));
    // Failed revalidation does not destroy the disk cache. Explicitly offline
    // callers may read it, with its unverified provenance clearly disclosed.
    let mut offline = Engine::new(
        &root,
        Options {
            fetch: false,
            upstream: false,
            private: false,
        },
    );
    offline.project.deps = vec![dep];
    let fallback = offline.docs(Some("cached-provenance-fixture"), "reject unknown keys", 1500).unwrap();
    assert!(fallback.text.contains("release_only_api") && fallback.text.contains("offline fallback"));
    assert!(fallback.text.contains("may have been a branch"));
    assert_eq!(fallback.json["provenance"][0]["upstream"]["format"], 3);
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&cache);
}

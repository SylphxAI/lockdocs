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
    Engine::new(&root, Options { fetch: false })
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

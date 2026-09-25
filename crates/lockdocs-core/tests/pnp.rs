//! Yarn Plug'n'Play: packages live only in `.yarn/cache/*.zip`.

use lockdocs_core::query::{Engine, Options};
use std::path::PathBuf;

#[test]
fn reads_packages_from_the_yarn_zip_cache() {
    let cache = std::env::temp_dir().join(format!("lockdocs-pnp-cache-{}", std::process::id()));
    std::env::set_var("LOCKDOCS_CACHE", &cache);
    std::env::set_var("YARN_CACHE_FOLDER", cache.join("no-global-yarn-cache"));
    std::env::set_var("LOCKDOCS_NO_SYSTEM_PYTHON", "1");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixture-pnp");
    let e = Engine::new(&root, Options { fetch: false });
    let r = e.resolve(Some("tiny-pnp"));
    assert!(r.text.contains("tiny-pnp@1.0.0") && r.text.contains("docs ready"), "{}", r.text);
    assert!(r.text.contains(".yarn/cache/tiny-pnp-npm-1.0.0-"), "{}", r.text);
    let a = e.api("route", Some("tiny-pnp"), 800).unwrap();
    assert!(a.text.contains("Registers a route for a path"), "{}", a.text);
    assert!(a.text.contains("tiny-pnp@1.0.0 index.d.ts:2"), "{}", a.text);
    let d = e.docs(Some("tiny-pnp"), "register a route", 800).unwrap();
    assert!(d.text.contains("README.md"), "{}", d.text);
    // The escaping entry in the zip was not written anywhere.
    assert!(!cache.join("src").join("npm").join("evil.md").exists());
    let _ = std::fs::remove_dir_all(&cache);
}

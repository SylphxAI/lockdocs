//! Cargo git dependencies: sources under `$CARGO_HOME/git/checkouts`.
//! Its own test binary, because CARGO_HOME is process-wide.

use lockdocs_core::query::{Engine, Options};
use std::path::PathBuf;

#[test]
fn finds_git_dependencies_in_cargo_checkouts() {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let cache = std::env::temp_dir().join(format!("lockdocs-git-cache-{}", std::process::id()));
    std::env::set_var("LOCKDOCS_CACHE", &cache);
    std::env::set_var("CARGO_HOME", tests.join("fixture-git-home"));
    std::env::set_var("LOCKDOCS_NO_SYSTEM_PYTHON", "1");
    std::env::set_var("LOCKDOCS_EMBED", "0");
    let mut e = Engine::new(&tests.join("fixture-git"), Options { fetch: true, upstream: true });
    let r = e.resolve(Some("tinygit"));
    assert!(r.text.contains("tinygit@0.2.0") && r.text.contains("docs ready"), "{}", r.text);
    let a = e.api("tinygit::clone_into", None, 800).unwrap();
    assert!(a.text.contains("Clones a repository into a directory"), "{}", a.text);
    assert!(a.text.contains("pinned in Cargo.lock (git)"), "{}", a.text);
    assert!(a.text.contains("git dependency's resolved commit"), "{}", a.text);
    let fetched = e.fetch_all(Some("tinygit")).unwrap();
    assert_eq!(fetched.json["packages"][0]["status"], "git-checkout");
    let uv = r#"[[package]]
name = "uv-git-fixture"
version = "1.2.3"
source = { git = "https://github.com/example/git-fixture?rev=abc#01234567" }
"#;
    e.project.deps = lockdocs_core::lockfile::uv_lock(uv).unwrap();
    let fetched = e.fetch_all(Some("uv-git-fixture")).unwrap();
    assert_eq!(fetched.json["packages"][0]["status"], "git-checkout");
    let _ = std::fs::remove_dir_all(&cache);
}

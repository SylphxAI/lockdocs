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
    let e = Engine::new(&tests.join("fixture-git"), Options { fetch: false });
    let r = e.resolve(Some("tinygit"));
    assert!(r.text.contains("tinygit@0.2.0") && r.text.contains("docs ready"), "{}", r.text);
    let a = e.api("tinygit::clone_into", None, 800).unwrap();
    assert!(a.text.contains("Clones a repository into a directory"), "{}", a.text);
    assert!(a.text.contains("pinned in Cargo.lock (git)"), "{}", a.text);
    let _ = std::fs::remove_dir_all(&cache);
}

//! The Pro commands exit 3 without a licence; free commands never do.

use std::process::Command;

fn run(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lockdocs"))
        .args(args)
        .env_remove("LOCKDOCS_LICENCE_TOKEN")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("LOCKDOCS_CACHE", home.join("cache"))
        .env("LOCKDOCS_OFFLINE", "1")
        .env("LOCKDOCS_NO_STAR_HINT", "1")
        .output()
        .unwrap()
}

#[test]
fn pro_commands_exit_3_and_free_commands_do_not() {
    let home = tempfile::tempdir().unwrap();
    for args in [&["upgrade", "zod", "4.0.0"][..], &["docs", "--pkg", "zod", "--upgrade-to", "4.0.0"][..]] {
        let o = run(args, home.path());
        assert_eq!(o.status.code(), Some(3), "{args:?}");
        assert!(String::from_utf8_lossy(&o.stderr).contains("lockdocs Pro"));
    }
    let o = run(&["licence", "status"], home.path());
    assert_eq!(o.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&o.stdout).contains("inactive"));
    // The placeholder key refuses every token, so activate fails (exit 1), never 3.
    let o = run(&["licence", "activate", "e30.AAAA"], home.path());
    assert_eq!(o.status.code(), Some(1));
    let o = run(&["version"], home.path());
    assert_eq!(o.status.code(), Some(0));
}

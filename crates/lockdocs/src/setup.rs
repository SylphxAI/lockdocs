//! `lockdocs setup`: register lockdocs with MCP clients (mcp-kit does the editing).

use anyhow::Result;
use mcp_kit::setup::{self, Options, Server};
use std::collections::HashMap;

pub fn run(flags: &HashMap<String, String>) -> Result<()> {
    let opts = Options {
        dry_run: flags.contains_key("dry-run"),
        remove: flags.contains_key("remove"),
        clients: flags.get("client").map(|c| c.split(',').map(|s| s.trim().to_string()).collect()),
    };
    println!("lockdocs setup{}", if opts.dry_run { " (dry run)" } else { "" });
    let mut args = vec!["mcp".to_string()];
    if flags.contains_key("fetch") {
        args.push("--fetch".into());
    }
    let server = Server {
        name: "lockdocs".into(),
        package: "@sylphx/lockdocs".into(),
        args,
    };
    if setup::run(&server, &opts)? > 0 && !opts.dry_run {
        println!("\nDone. Restart your editor or agent, then ask it: \"Use lockdocs: which version of <library> do we use, and how do I ...?\"");
    }
    Ok(())
}

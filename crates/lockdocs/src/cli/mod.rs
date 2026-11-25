mod args;
mod commands;
mod help;
mod upgrade;

use self::{args::Args, help::HELP};
use crate::{login, mcp, pro};
use anyhow::{bail, Result};
use lockdocs_core::query::Options;
use std::io::IsTerminal;

pub(super) fn run() -> Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.is_empty() {
        if std::io::stdin().is_terminal() {
            println!("{HELP}");
            return Ok(());
        }
        std::thread::spawn(|| ensure_model(false));
        return mcp::serve(None, Options::default());
    }
    // The licence CLI owns its argument grammar; forward it unchanged.
    if matches!(raw[0].as_str(), "licence" | "license") {
        std::process::exit(pro::run_licence(&raw[1..]));
    }
    // Select the command after parsing options and their values. A leading
    // global option does not mean flags-only when a CLI command follows it.
    let mut args = Args::parse(raw);
    let cmd = args.take_command();
    if cmd == "login" {
        if !args.positional.is_empty() {
            bail!("usage: lockdocs login [--test] [--no-browser]");
        }
        if args.on("offline") || std::env::var("LOCKDOCS_OFFLINE").is_ok_and(|v| v != "0") {
            bail!("login needs network access; use `lockdocs licence activate <token>` to activate offline");
        }
        return login::run(args.on("test"), args.on("no-browser"));
    }
    commands::dispatch(&cmd, &args)
}

fn ensure_model(offline: bool) {
    use lockdocs_core::embed;
    if offline || std::env::var("LOCKDOCS_OFFLINE").is_ok_and(|v| v != "0") || !embed::enabled() || embed::installed() {
        return;
    }
    if let Err(e) = embed::ensure() {
        eprintln!("lockdocs: embedding model unavailable ({e:#}); using keyword search only.");
    }
}

use super::{args::Args, ensure_model, help::HELP, upgrade};
use crate::{mcp, pro, setup, VERSION};
use anyhow::{bail, Result};
use lockdocs_core::query::{Answer, Engine};
use std::path::PathBuf;

pub(super) fn dispatch(cmd: &str, args: &Args) -> Result<()> {
    let upgrade_target = upgrade::authorize(cmd, args);
    // Query commands fetch the embedding model once (BM25-only if that fails).
    if !matches!(
        cmd,
        "mcp"
            | "serve"
            | "help"
            | "--help"
            | "-h"
            | "version"
            | "--version"
            | "-V"
            | "setup"
            | "cache"
            | "resolve"
            | "versions"
            | "deps"
            | "fetch"
            | "pull"
            | "licence"
            | "license"
    ) {
        ensure_model(args.on("offline"));
    }
    let json = args.on("json");
    // Private registries and git hosts are lockdocs Pro: the licence is checked
    // here, before any engine can send a request to one.
    let private = pro::require(&pro::POLICY, pro::PRIVATE_SOURCES);
    let cell: std::cell::OnceCell<Engine> = std::cell::OnceCell::new();
    let engine = || {
        cell.get_or_init(|| {
            let mut opts = args.opts();
            opts.private = private.is_ok();
            Engine::new(&args.root(), opts)
        })
    };
    let counts_as_run = !json
        && !matches!(
            cmd,
            "mcp" | "serve" | "help" | "--help" | "-h" | "version" | "--version" | "-V" | "setup" | "cache" | "fetch" | "pull"
        );
    let result = match cmd {
        "mcp" | "serve" => {
            let offline = args.on("offline");
            // Fetch the model in the background; queries are keyword-only until it lands.
            std::thread::spawn(move || ensure_model(offline));
            mcp::serve(args.flag("root").or_else(|| args.flag("C")).map(PathBuf::from), args.opts())
        }
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("lockdocs {VERSION}");
            Ok(())
        }
        "setup" => setup::run(&args.flags),
        "resolve" | "versions" | "deps" => print(engine().resolve(args.positional.first().map(|s| s.as_str())), json),
        "upgrade" => {
            let (Some(pkg), Some(target)) = (args.positional.first(), upgrade_target) else {
                bail!("usage: lockdocs upgrade <package> <target version>");
            };
            print(engine().upgrade_report(pkg, target, args.tokens()).map_err(anyhow::Error::msg)?, json)
        }
        "docs" | "search" if upgrade_target.is_some() => {
            let Some(pkg) = args.pkg() else {
                bail!("usage: lockdocs docs --pkg <package> --upgrade-to <version>");
            };
            print(
                engine()
                    .upgrade_report(pkg, upgrade_target.unwrap_or_default(), args.tokens())
                    .map_err(anyhow::Error::msg)?,
                json,
            )
        }
        "docs" | "search" => {
            let q = args.positional.join(" ");
            if q.trim().is_empty() && args.pkg().is_none() {
                bail!("usage: lockdocs docs <question> [--pkg <package>]");
            }
            print(engine().docs(args.pkg(), &q, args.tokens()).map_err(anyhow::Error::msg)?, json)
        }
        "api" => {
            let sym = args.positional.join(" ");
            if sym.trim().is_empty() {
                bail!("usage: lockdocs api <symbol> [--pkg <package>]");
            }
            print(engine().api(&sym, args.pkg(), args.tokens()).map_err(anyhow::Error::msg)?, json)
        }
        "index" | "warm" => {
            let p = if args.positional.is_empty() { None } else { Some(args.positional.join(",")) };
            print(engine().warm(p.as_deref().or(args.pkg())).map_err(anyhow::Error::msg)?, json)
        }
        "cache" => cache(args),
        "fetch" | "pull" => {
            if args.on("offline") || std::env::var("LOCKDOCS_OFFLINE").is_ok_and(|v| v != "0") {
                bail!("fetch cannot download with --offline; use docs/api to read cached files");
            }
            let p = if args.positional.is_empty() { None } else { Some(args.positional.join(",")) };
            print(engine().fetch_all(p.as_deref().or(args.pkg())).map_err(anyhow::Error::msg)?, json)
        }
        _ => {
            // `lockdocs <package> [question...]`
            let q = args.positional.join(" ");
            let e = engine();
            match e.docs(Some(cmd), &q, args.tokens()) {
                Ok(a) => print(a, json),
                Err(err)
                    if lockdocs_core::project::Spec::parse(cmd).version.is_none()
                        && e.project.find(&lockdocs_core::project::Spec::parse(cmd)).is_empty()
                        && !cmd.contains(':') =>
                {
                    bail!("{err}\nRun `lockdocs help` for commands.")
                }
                Err(err) => bail!("{err}"),
            }
        }
    };
    if let (Err(required), Some(e)) = (&private, cell.get()) {
        let blocked = e.take_private_blocked();
        if !blocked.is_empty() {
            let names: Vec<String> = blocked.iter().map(|b| b.package.clone()).collect();
            if result.is_err() || args.pkg().is_some() || matches!(cmd, "fetch" | "pull") {
                eprintln!("{required}");
                eprintln!("Needs a private source: {}", names.join(", "));
                std::process::exit(3);
            }
            eprintln!("{} need a private source, which is part of lockdocs Pro: {}", names.join(", "), required.url);
        }
    }
    if result.is_ok() && counts_as_run {
        mcp_kit::star_hint::after_success(
            "Enjoying lockdocs? A GitHub star helps others find it: https://github.com/SylphxAI/lockdocs",
            "LOCKDOCS_NO_STAR_HINT",
            &lockdocs_core::cache::dir(),
            false,
        );
    }
    result
}

fn print(a: Answer, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&a.json)?);
    } else {
        print!("{}", a.text);
        if !a.text.ends_with('\n') {
            println!();
        }
    }
    Ok(())
}

fn cache(args: &Args) -> Result<()> {
    let dir = lockdocs_core::cache::dir();
    if args.positional.first().map(|s| s.as_str()) == Some("clean") {
        if dir.is_dir() {
            std::fs::remove_dir_all(&dir)?;
        }
        println!("Removed {}", dir.display());
        return Ok(());
    }
    let (bytes, files) = lockdocs_core::cache::usage();
    println!(
        "{}\n{} files, {:.1} MB (indexes and fetched packages). `lockdocs cache clean` removes it.",
        dir.display(),
        files,
        bytes as f64 / 1_048_576.0
    );
    Ok(())
}

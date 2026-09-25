mod mcp;
mod setup;
mod tools;

use anyhow::{bail, Result};
use lockdocs_core::query::{Answer, Engine, Options, DEFAULT_TOKENS};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "lockdocs — exact-version library docs for AI agents, straight from your lockfile

Usage:
  lockdocs <package> [question]   Docs for the version this project pins (no question: overview)
  lockdocs <command> [options]

Commands:
  setup                 Add lockdocs to Claude Code, Codex, Cursor, VS Code, Claude Desktop,
                        Windsurf and Gemini CLI (--client a,b  --dry-run  --remove  --fetch)
  resolve [filter]      Pinned versions and where their docs are (filter searches transitive deps)
  docs <question>       Search the docs of all direct dependencies (--pkg to focus)
  api <symbol>          Exact signature + doc: z.object, tokio::spawn, BaseModel.model_dump
  fetch [package...]    Download, once, what makes answers complete: upstream docs at each
                        version's git tag, missing packages, and the embedding model
  index [package]       Build the indexes ahead of time and show what they hold
  cache [clean]         Show or delete the cache (indexes and fetched packages)
  mcp                   Run the MCP server on stdio (default when stdin is not a terminal)
  version               Print the version

Options:
  -C, --root <dir>      Project directory (default: current directory)
  --pkg <package>       Package for docs/api: zod, npm:zod, pydantic@2.9.2, tokio
  --tokens <n>          Answer budget in tokens (default 1200)
  --fetch               Allow downloads during queries: missing packages and upstream docs
                        (cached; also LOCKDOCS_FETCH=1). Without it, only `lockdocs fetch`
                        and the one-time model download use the network
  --offline             Never download (not even the embedding model)
  --json                Machine-readable output

Examples:
  lockdocs zod \"strict object unknown keys\"
  lockdocs api axum::Router::route
  lockdocs pydantic \"serialize model to dict\"
  lockdocs resolve

Docs: https://sylphxai.github.io/lockdocs/";

struct Args {
    positional: Vec<String>,
    flags: HashMap<String, String>,
}

impl Args {
    fn parse(raw: Vec<String>) -> Args {
        let mut positional = Vec::new();
        let mut flags = HashMap::new();
        let takes_value = ["root", "C", "pkg", "package", "tokens", "client"];
        let mut it = raw.into_iter();
        while let Some(a) = it.next() {
            if a == "--" {
                positional.extend(it.by_ref());
                break;
            }
            if let Some(name) = a.strip_prefix("--").or_else(|| a.strip_prefix('-').filter(|n| n.len() == 1)) {
                if let Some((k, v)) = name.split_once('=') {
                    flags.insert(k.to_string(), v.to_string());
                } else if takes_value.contains(&name) {
                    flags.insert(name.to_string(), it.next().unwrap_or_default());
                } else {
                    flags.insert(name.to_string(), "true".into());
                }
            } else {
                positional.push(a);
            }
        }
        Args { positional, flags }
    }
    fn flag(&self, k: &str) -> Option<&str> {
        self.flags.get(k).map(|s| s.as_str())
    }
    fn on(&self, k: &str) -> bool {
        self.flags.get(k).is_some_and(|v| v != "false")
    }
    fn root(&self) -> PathBuf {
        self.flag("root")
            .or_else(|| self.flag("C"))
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
    fn tokens(&self) -> usize {
        self.flag("tokens").and_then(|t| t.parse().ok()).unwrap_or(DEFAULT_TOKENS)
    }
    fn pkg(&self) -> Option<&str> {
        self.flag("pkg").or_else(|| self.flag("package"))
    }
    fn opts(&self) -> Options {
        let mut o = Options::default();
        if self.on("fetch") {
            o.fetch = true;
        }
        if self.on("offline") {
            o.fetch = false;
        }
        o
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("lockdocs: {e:#}");
        std::process::exit(1);
    }
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

fn run() -> Result<()> {
    let mut raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.is_empty() {
        if std::io::stdin().is_terminal() {
            println!("{HELP}");
            return Ok(());
        }
        std::thread::spawn(|| ensure_model(false));
        return mcp::serve(None, Options::default());
    }
    if raw[0].starts_with('-') && !matches!(raw[0].as_str(), "-h" | "--help" | "-V" | "--version") {
        // Flags only (`lockdocs --fetch`): the MCP server.
        raw.insert(0, "mcp".into());
    }
    let cmd = raw.remove(0);
    let args = Args::parse(raw);
    // Query commands fetch the embedding model once (BM25-only if that fails).
    if !matches!(
        cmd.as_str(),
        "mcp" | "serve" | "help" | "--help" | "-h" | "version" | "--version" | "-V" | "setup" | "cache" | "resolve" | "versions" | "deps" | "fetch" | "pull"
    ) {
        ensure_model(args.on("offline"));
    }
    let json = args.on("json");
    let engine = || Engine::new(&args.root(), args.opts());
    match cmd.as_str() {
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
        "cache" => cache(&args),
        "fetch" | "pull" => {
            let p = if args.positional.is_empty() { None } else { Some(args.positional.join(",")) };
            print(engine().fetch_all(p.as_deref().or(args.pkg())).map_err(anyhow::Error::msg)?, json)
        }
        _ => {
            // `lockdocs <package> [question...]`
            let q = args.positional.join(" ");
            let e = engine();
            match e.docs(Some(&cmd), &q, args.tokens()) {
                Ok(a) => print(a, json),
                Err(err)
                    if lockdocs_core::project::Spec::parse(&cmd).version.is_none()
                        && e.project.find(&lockdocs_core::project::Spec::parse(&cmd)).is_empty()
                        && !cmd.contains(':') =>
                {
                    bail!("{err}\nRun `lockdocs help` for commands.")
                }
                Err(err) => bail!("{err}"),
            }
        }
    }
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

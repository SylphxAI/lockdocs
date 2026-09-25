//! `lockdocs setup`: detect MCP clients and register lockdocs with each one.
//! Idempotent; prints exactly what changed.

use anyhow::Result;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const NAME: &str = "lockdocs";

fn launch(fetch: bool) -> (String, Vec<String>) {
    let mut base = vec!["-y".to_string(), "@sylphx/lockdocs".to_string(), "mcp".to_string()];
    if fetch {
        base.push("--fetch".to_string());
    }
    if cfg!(windows) {
        let mut a = vec!["/c".to_string(), "npx".to_string()];
        a.extend(base);
        ("cmd".into(), a)
    } else {
        ("npx".into(), base)
    }
}

#[derive(Clone, Copy)]
enum Format {
    /// `{ "mcpServers": { name: {command, args} } }`
    McpServers,
    /// VS Code: `{ "servers": { name: {type, command, args} } }`
    VsCode,
    /// Claude Code user config: mcpServers with `type: stdio`
    ClaudeJson,
}

struct Client {
    id: &'static str,
    label: &'static str,
    detect: PathBuf,
    config: PathBuf,
    format: Option<Format>,
}

fn on_path(bin: &str) -> bool {
    let exts: Vec<&str> = if cfg!(windows) { vec![".exe", ".cmd", ".bat", ""] } else { vec![""] };
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| exts.iter().any(|e| d.join(format!("{bin}{e}")).is_file())))
}

fn clients() -> Vec<Client> {
    let home = dirs::home_dir().unwrap_or_default();
    let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
    let mut v = vec![
        Client {
            id: "claude-code",
            label: "Claude Code",
            detect: home.join(".claude"),
            config: home.join(".claude.json"),
            format: Some(Format::ClaudeJson),
        },
        Client {
            id: "codex",
            label: "Codex",
            detect: home.join(".codex"),
            config: home.join(".codex").join("config.toml"),
            format: None,
        },
        Client {
            id: "cursor",
            label: "Cursor",
            detect: home.join(".cursor"),
            config: home.join(".cursor").join("mcp.json"),
            format: Some(Format::McpServers),
        },
        Client {
            id: "vscode",
            label: "VS Code",
            detect: config.join("Code").join("User"),
            config: config.join("Code").join("User").join("mcp.json"),
            format: Some(Format::VsCode),
        },
        Client {
            id: "vscode-insiders",
            label: "VS Code Insiders",
            detect: config.join("Code - Insiders").join("User"),
            config: config.join("Code - Insiders").join("User").join("mcp.json"),
            format: Some(Format::VsCode),
        },
        Client {
            id: "claude-desktop",
            label: "Claude Desktop",
            detect: config.join("Claude"),
            config: config.join("Claude").join("claude_desktop_config.json"),
            format: Some(Format::McpServers),
        },
        Client {
            id: "windsurf",
            label: "Windsurf",
            detect: home.join(".codeium").join("windsurf"),
            config: home.join(".codeium").join("windsurf").join("mcp_config.json"),
            format: Some(Format::McpServers),
        },
        Client {
            id: "gemini",
            label: "Gemini CLI",
            detect: home.join(".gemini"),
            config: home.join(".gemini").join("settings.json"),
            format: Some(Format::McpServers),
        },
    ];
    // Codex may be installed without a config dir yet.
    if on_path("codex") {
        v[1].detect = home.clone();
    }
    v
}

pub fn run(flags: &HashMap<String, String>) -> Result<()> {
    let dry = flags.contains_key("dry-run");
    let remove = flags.contains_key("remove");
    let only: Option<Vec<String>> = flags.get("client").map(|c| c.split(',').map(|s| s.trim().to_string()).collect());
    let (cmd, args) = launch(flags.contains_key("fetch"));
    let mut touched = 0;
    let mut found = 0;
    println!("lockdocs setup{}", if dry { " (dry run)" } else { "" });
    for c in clients() {
        if let Some(only) = &only {
            if !only.iter().any(|o| o == c.id) {
                continue;
            }
        } else if !c.detect.exists() {
            continue;
        }
        found += 1;
        let res = if c.id == "claude-code" && on_path("claude") {
            claude_cli(&cmd, &args, dry, remove)
        } else {
            match c.format {
                Some(fmt) => edit_json(&c.config, fmt, &cmd, &args, dry, remove),
                None => edit_codex(&c.config, &cmd, &args, dry, remove),
            }
        };
        match res {
            Ok(Change::Unchanged) => println!("  = {:<17} already configured ({})", c.label, c.config.display()),
            Ok(Change::Wrote(what)) => {
                touched += 1;
                let what = if dry {
                    format!("would be {}", what.replace("wrote", "written to"))
                } else {
                    what.to_string()
                };
                println!("  + {:<17} {} {}", c.label, what, c.config.display());
            }
            Err(e) => println!("  ! {:<17} {e}", c.label),
        }
    }
    if found == 0 {
        println!("  No MCP clients detected. Add this to your client's MCP config:");
        println!(
            "  {{\"mcpServers\": {{\"lockdocs\": {{\"command\": \"{cmd}\", \"args\": {}}}}}}}",
            serde_json::to_string(&args)?
        );
    } else if touched > 0 && !dry {
        println!("\nDone. Restart your editor or agent, then ask it: \"Use lockdocs: which version of <library> do we use, and how do I ...?\"");
    }
    Ok(())
}

enum Change {
    Unchanged,
    Wrote(&'static str),
}

fn entry(fmt: Format, cmd: &str, args: &[String]) -> Value {
    match fmt {
        Format::McpServers => json!({"command": cmd, "args": args}),
        Format::VsCode | Format::ClaudeJson => json!({"type": "stdio", "command": cmd, "args": args}),
    }
}

fn edit_json(path: &Path, fmt: Format, cmd: &str, args: &[String], dry: bool, remove: bool) -> Result<Change> {
    let mut root: Value = if path.exists() {
        let txt = std::fs::read_to_string(path)?;
        if txt.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&txt).map_err(|e| anyhow::anyhow!("cannot parse {} ({e}); add the server manually", path.display()))?
        }
    } else {
        json!({})
    };
    let key = if matches!(fmt, Format::VsCode) { "servers" } else { "mcpServers" };
    let obj = root.as_object_mut().ok_or_else(|| anyhow::anyhow!("{} is not a JSON object", path.display()))?;
    let servers = obj.entry(key).or_insert_with(|| Value::Object(Map::new()));
    let servers = servers.as_object_mut().ok_or_else(|| anyhow::anyhow!("`{key}` is not an object"))?;
    let want = entry(fmt, cmd, args);
    if remove {
        if servers.remove(NAME).is_none() {
            return Ok(Change::Unchanged);
        }
    } else {
        if servers.get(NAME) == Some(&want) {
            return Ok(Change::Unchanged);
        }
        servers.insert(NAME.into(), want);
    }
    if !dry {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("lockdocs-tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&root)? + "\n")?;
        std::fs::rename(&tmp, path)?;
    }
    Ok(Change::Wrote(if remove { "removed from" } else { "wrote" }))
}

fn edit_codex(path: &Path, cmd: &str, args: &[String], dry: bool, remove: bool) -> Result<Change> {
    use toml_edit::{value, Array, DocumentMut, Item, Table};
    let txt = if path.exists() { std::fs::read_to_string(path)? } else { String::new() };
    let mut doc: DocumentMut = txt.parse().map_err(|e| anyhow::anyhow!("cannot parse {}: {e}", path.display()))?;
    if !doc.contains_key("mcp_servers") {
        let mut t = Table::new();
        t.set_implicit(true);
        doc["mcp_servers"] = Item::Table(t);
    }
    let servers = doc["mcp_servers"].as_table_mut().ok_or_else(|| anyhow::anyhow!("mcp_servers is not a table"))?;
    if remove {
        if servers.remove(NAME).is_none() {
            return Ok(Change::Unchanged);
        }
    } else {
        let existing = servers.get(NAME).and_then(|t| t.as_table());
        let same = existing.is_some_and(|t| {
            t.get("command").and_then(|c| c.as_str()) == Some(cmd)
                && t.get("args")
                    .and_then(|a| a.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).collect::<Vec<_>>())
                    == Some(args.to_vec())
        });
        if same {
            return Ok(Change::Unchanged);
        }
        let mut t = Table::new();
        t["command"] = value(cmd);
        let mut arr = Array::new();
        for a in args {
            arr.push(a.as_str());
        }
        t["args"] = value(arr);
        servers.insert(NAME, Item::Table(t));
    }
    if !dry {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, doc.to_string())?;
    }
    Ok(Change::Wrote(if remove { "removed from" } else { "wrote" }))
}

fn claude_cli(cmd: &str, args: &[String], dry: bool, remove: bool) -> Result<Change> {
    let exists = Command::new("claude")
        .args(["mcp", "get", NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if remove {
        if !exists {
            return Ok(Change::Unchanged);
        }
        if !dry {
            run_ok(Command::new("claude").args(["mcp", "remove", "--scope", "user", NAME]))?;
        }
        return Ok(Change::Wrote("removed (claude mcp remove) from"));
    }
    if exists {
        return Ok(Change::Unchanged);
    }
    if !dry {
        let mut c = Command::new("claude");
        c.args(["mcp", "add", "--scope", "user", NAME, "--", cmd]).args(args);
        run_ok(&mut c)?;
    }
    Ok(Change::Wrote("added (claude mcp add --scope user) to"))
}

fn run_ok(c: &mut Command) -> Result<()> {
    let out = c.output()?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_edit_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("lockdocs-setup-{}", std::process::id()));
        let path = dir.join("mcp.json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"x"}}}"#).unwrap();
        let args = vec!["-y".to_string(), "@sylphx/lockdocs".to_string(), "mcp".to_string()];
        assert!(matches!(
            edit_json(&path, Format::McpServers, "npx", &args, false, false).unwrap(),
            Change::Wrote(_)
        ));
        assert!(matches!(
            edit_json(&path, Format::McpServers, "npx", &args, false, false).unwrap(),
            Change::Unchanged
        ));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["mcpServers"]["lockdocs"]["command"], "npx");
        let toml = dir.join("config.toml");
        std::fs::write(&toml, "model = \"o3\"\n").unwrap();
        assert!(matches!(edit_codex(&toml, "npx", &args, false, false).unwrap(), Change::Wrote(_)));
        assert!(matches!(edit_codex(&toml, "npx", &args, false, false).unwrap(), Change::Unchanged));
        let t = std::fs::read_to_string(&toml).unwrap();
        assert!(t.contains("model = \"o3\"") && t.contains("[mcp_servers.lockdocs]"), "{t}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

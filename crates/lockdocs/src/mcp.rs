//! MCP server over stdio (newline-delimited JSON-RPC 2.0).

use crate::tools;
use lockdocs_core::query::{Options, Workspace};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const PROTOCOLS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "lockdocs answers from the exact dependency versions pinned in this project's lockfiles, using the installed packages' own docs and type declarations, offline. Call `resolve` to see versions, `docs` with a question (and optionally `package`) before using an API you are unsure of in this version, and `api` for the exact signature of a symbol like `z.object` or `tokio::spawn`. Every section cites package@version path:line.";

struct Roots {
    list: Mutex<Option<Vec<PathBuf>>>,
    ready: Condvar,
}

pub fn serve(default_root: Option<PathBuf>, opts: Options) -> anyhow::Result<()> {
    let opts = Arc::new(opts);
    let ws = Arc::new(Workspace::default());
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let roots = Arc::new(Roots {
        list: Mutex::new(None),
        ready: Condvar::new(),
    });
    let mut client_roots = false;

    let send = {
        let out = out.clone();
        move |v: Value| {
            let mut o = out.lock().unwrap();
            let _ = writeln!(o, "{v}");
            let _ = o.flush();
        }
    };

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                send(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}));
                continue;
            }
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");

        // Responses to our own requests (roots/list).
        if method.is_empty() {
            if id.as_ref().and_then(|v| v.as_str()).is_some_and(|s| s.starts_with("roots")) {
                let list: Vec<PathBuf> = msg
                    .pointer("/result/roots")
                    .and_then(|r| r.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| r.get("uri").and_then(|u| u.as_str()))
                            .filter_map(file_uri_to_path)
                            .collect()
                    })
                    .unwrap_or_default();
                *roots.list.lock().unwrap() = Some(list);
                roots.ready.notify_all();
            }
            continue;
        }

        match method {
            "initialize" => {
                let asked = msg.pointer("/params/protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOLS[0]);
                let version = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[0] };
                client_roots = msg.pointer("/params/capabilities/roots").is_some();
                if !client_roots {
                    *roots.list.lock().unwrap() = Some(Vec::new());
                }
                send(json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "lockdocs", "title": "lockdocs", "version": crate::VERSION, "websiteUrl": "https://sylphxai.github.io/lockdocs/"},
                    "instructions": INSTRUCTIONS
                }}));
            }
            "notifications/initialized" | "notifications/roots/list_changed" => {
                if client_roots {
                    if method == "notifications/roots/list_changed" {
                        *roots.list.lock().unwrap() = None;
                    }
                    send(json!({"jsonrpc": "2.0", "id": format!("roots-{}", rand_id()), "method": "roots/list"}));
                }
                // Read the lockfiles in the background so the first call is fast.
                let ws = ws.clone();
                let opts = opts.clone();
                let roots = roots.clone();
                let default_root = default_root.clone();
                std::thread::spawn(move || {
                    if let Ok(root) = pick_root(None, default_root.as_ref(), &roots) {
                        let _ = ws.engine(&root, &opts);
                    }
                });
            }
            "ping" => send(json!({"jsonrpc": "2.0", "id": id, "result": {}})),
            "tools/list" => send(json!({"jsonrpc": "2.0", "id": id, "result": {"tools": tools::definitions()}})),
            "tools/call" => {
                let ws = ws.clone();
                let opts = opts.clone();
                let roots = roots.clone();
                let default_root = default_root.clone();
                let send = send.clone();
                std::thread::spawn(move || {
                    let name = msg.pointer("/params/name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let args = msg.pointer("/params/arguments").cloned().unwrap_or(json!({}));
                    let explicit = args.get("root").or_else(|| args.get("repo_root")).and_then(|v| v.as_str()).map(PathBuf::from);
                    let result = pick_root(explicit, default_root.as_ref(), &roots).and_then(|root| tools::call(&ws, &opts, &name, &args, &root));
                    let want_json = args.get("format").and_then(|v| v.as_str()) == Some("json");
                    let body = match result {
                        Ok(o) => {
                            let text = if want_json {
                                serde_json::to_string_pretty(&o.json).unwrap_or_default()
                            } else {
                                o.text
                            };
                            json!({"content": [{"type": "text", "text": text}], "isError": false})
                        }
                        Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
                    };
                    send(json!({"jsonrpc": "2.0", "id": id, "result": body}));
                });
            }
            "resources/list" => send(json!({"jsonrpc": "2.0", "id": id, "result": {"resources": []}})),
            "prompts/list" => send(json!({"jsonrpc": "2.0", "id": id, "result": {"prompts": []}})),
            m if m.starts_with("notifications/") => {}
            _ => {
                if id.is_some() {
                    send(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("method not found: {method}")}}));
                }
            }
        }
    }
    Ok(())
}

fn rand_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

fn pick_root(explicit: Option<PathBuf>, default_root: Option<&PathBuf>, roots: &Roots) -> Result<PathBuf, String> {
    if let Some(r) = explicit {
        return check(r);
    }
    for var in ["LOCKDOCS_ROOT"] {
        if let Ok(r) = std::env::var(var) {
            if !r.is_empty() {
                return check(PathBuf::from(r));
            }
        }
    }
    if let Some(r) = default_root {
        return check(r.clone());
    }
    // Wait briefly for the client's roots answer.
    let guard = roots.list.lock().unwrap();
    let (guard, _) = roots.ready.wait_timeout_while(guard, Duration::from_secs(3), |l| l.is_none()).unwrap();
    if let Some(first) = guard.as_ref().and_then(|l| l.first()) {
        return check(first.clone());
    }
    drop(guard);
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let home = dirs::home_dir();
    if cwd.parent().is_none() || Some(&cwd) == home.as_ref() {
        return Err("No project selected: pass `root` (absolute path to the project), or start the server from the project directory.".into());
    }
    check(cwd)
}

fn check(p: PathBuf) -> Result<PathBuf, String> {
    if p.is_dir() {
        Ok(p)
    } else {
        Err(format!("root `{}` is not a directory", p.display()))
    }
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(rest.len());
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    let s = String::from_utf8(bytes).ok()?;
    // file:///C:/x on Windows
    let s = if s.len() > 3 && s.as_bytes()[0] == b'/' && s.as_bytes()[2] == b':' {
        s[1..].to_string()
    } else {
        s
    };
    Some(PathBuf::from(s))
}

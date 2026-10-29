//! Global options must not turn a CLI query into a stdio server.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lockdocs-core/tests/fixture")
        .canonicalize()
        .unwrap()
}

fn command(home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_lockdocs"));
    c.env_remove("LOCKDOCS_LICENCE_TOKEN")
        .env_remove("LOCKDOCS_OFFLINE")
        .env_remove("LOCKDOCS_NO_UPSTREAM")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("LOCKDOCS_CACHE", home.join("cache"))
        .env("LOCKDOCS_NO_SYSTEM_PYTHON", "1")
        .env("LOCKDOCS_NO_STAR_HINT", "1")
        .env("LOCKDOCS_EMBED", "1")
        .env("LOCKDOCS_FETCH", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn output(mut c: Command, open_stdin: bool) -> Output {
    if open_stdin {
        c.stdin(Stdio::piped());
    }
    let mut p = Process(c.spawn().unwrap());
    let read = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).unwrap();
            bytes
        })
    };
    let stdout = read(Box::new(p.0.stdout.take().unwrap()));
    let stderr = read(Box::new(p.0.stderr.take().unwrap()));
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = p.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "CLI waited for input: {c:?}");
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

fn query(args: &[&str], before: &[&str], open_stdin: bool) -> Output {
    let home = tempfile::tempdir().unwrap();
    let mut c = command(home.path());
    c.args(before).args(args);
    if !before.iter().any(|s| *s == "-C" || *s == "--root" || s.starts_with("--root=")) {
        c.args(["-C", fixture().to_str().unwrap()]);
    }
    for flag in ["--offline", "--json"] {
        if !before.contains(&flag) {
            c.arg(flag);
        }
    }
    output(c, open_stdin)
}

fn successful_json(o: &Output) -> Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).expect("one machine-readable CLI answer, not MCP messages")
}

#[test]
fn global_options_before_queries_match_command_first_with_closed_or_open_stdin() {
    let root = fixture();
    for args in [
        vec!["resolve"],
        vec!["tiny-schema", "reject unknown keys"],
        vec!["docs", "reject unknown keys", "--pkg", "tiny-schema"],
        vec!["api", "s.object", "--pkg", "tiny-schema"],
    ] {
        let expected = successful_json(&query(&args, &[], false));
        if args[0] != "resolve" {
            let text = expected.to_string();
            assert!(text.contains("1.2.0") && (text.contains("README.md") || text.contains("index.d.ts")), "{text}");
        }
        for before in [
            vec!["--json"],
            vec!["--offline"],
            vec!["-C", root.to_str().unwrap()],
            vec!["--root", root.to_str().unwrap(), "--tokens", "1200", "--offline", "--json"],
        ] {
            for open in [false, true] {
                assert_eq!(successful_json(&query(&args, &before, open)), expected, "{before:?} {args:?}");
            }
        }
    }
    let resolved = successful_json(&query(&["resolve"], &["--json"], false));
    for pinned in ["tiny-schema", "1.2.0", "tinyrt", "0.3.0", "tinymodel", "tinyweb", "missing-pkg"] {
        assert!(resolved.to_string().contains(pinned), "{resolved}");
    }
}

#[test]
fn unknown_dependency_is_an_actionable_cli_error_in_both_orders() {
    for before in [&[][..], &["--offline"][..], &["--json"][..]] {
        let o = query(&["not-a-project-dependency", "how to use it"], before, false);
        assert_eq!(o.status.code(), Some(1));
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(
            err.contains("not a dependency") && err.contains("Use `resolve`") && err.contains("lockdocs help"),
            "{err}"
        );
        assert!(!err.contains("initialize"), "{err}");
    }
}

#[test]
fn offline_prevents_model_package_and_upstream_requests_in_both_orders() {
    // Capture every attempted HTTP(S) request, including the model's own URL.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (stop, stopped) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut requests = 0;
        while stopped.try_recv().is_err() {
            if let Ok((mut socket, _)) = listener.accept() {
                requests += 1;
                let _ = socket.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        requests
    });
    for before in [false, true] {
        for args in [
            vec!["tiny-schema", "reject unknown keys"],
            vec!["api", "s.object", "--pkg", "tiny-schema"],
            vec!["missing-pkg", "overview"],
            vec!["fetch"],
        ] {
            let home = tempfile::tempdir().unwrap();
            let mut c = command(home.path());
            c.env("HTTP_PROXY", &url)
                .env("HTTPS_PROXY", &url)
                .env("ALL_PROXY", &url)
                .env("NO_PROXY", "")
                .env("LOCKDOCS_MODEL_URL", &url);
            if before {
                c.arg("--offline");
            }
            c.args(&args).args(["-C", fixture().to_str().unwrap(), "--fetch", "--json"]);
            if !before {
                c.arg("--offline");
            }
            let o = output(c, false);
            let err = String::from_utf8_lossy(&o.stderr);
            if args[0] == "missing-pkg" {
                assert!(!o.status.success() && err.contains("not on this machine"), "{err}");
            } else if args[0] == "fetch" {
                assert!(!o.status.success() && err.contains("fetch cannot download with --offline"), "{err}");
            } else {
                successful_json(&o);
            }
            assert!(!home.path().join("cache/models").exists());
            assert!(!err.contains("embedding model unavailable"), "{err}");
        }
    }
    stop.send(()).unwrap();
    assert_eq!(server.join().unwrap(), 0, "--offline must override every download opt-in");
}

#[test]
fn informational_commands_and_setup_remain_cli_commands() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["help"],
        vec!["--help"],
        vec!["-h"],
        vec!["version"],
        vec!["--version"],
        vec!["-V"],
        vec!["--offline", "version"],
        vec!["setup", "--client", "cursor,codex", "--dry-run"],
    ] {
        let mut c = command(home.path());
        c.args(&args);
        let o = output(c, false);
        assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
        assert!(!o.stdout.is_empty());
    }
}

#[test]
fn explicit_flags_only_and_no_argument_mcp_keep_the_three_query_tools() {
    for args in [vec!["mcp", "--offline", "-C"], vec!["--offline", "-C"], vec!["--offline", "mcp", "-C"], vec![]] {
        let home = tempfile::tempdir().unwrap();
        let mut c = command(home.path());
        c.args(&args).env("LOCKDOCS_OFFLINE", "1").current_dir(fixture()).stdin(Stdio::piped());
        if !args.is_empty() {
            c.arg(fixture());
        }
        let mut p = Process(c.spawn().unwrap());
        let mut input = p.0.stdin.take().unwrap();
        let reader = BufReader::new(p.0.stdout.take().unwrap());
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            for line in reader.lines() {
                if send.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let mut request = |value: Value| {
            writeln!(input, "{value}").unwrap();
            input.flush().unwrap();
            if value.get("id").is_none() {
                return Value::Null;
            }
            serde_json::from_str::<Value>(&receive.recv_timeout(Duration::from_secs(15)).expect("MCP response")).unwrap()
        };
        let initialized = request(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        );
        assert_eq!(initialized["result"]["serverInfo"]["name"], "lockdocs");
        request(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        let tools = request(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        let mut names: Vec<_> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["api", "docs", "resolve"]);
        for (id, name, arguments, expected) in [
            (3, "resolve", json!({}), "tinyrt@0.3.0"),
            (
                4,
                "docs",
                json!({"package":"tiny-schema","query":"reject unknown keys"}),
                "tiny-schema@1.2.0 README.md:9",
            ),
            (5, "api", json!({"package":"tiny-schema","symbol":"s.object"}), "tiny-schema@1.2.0 index.d.ts:"),
        ] {
            let answer = request(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}}));
            assert_ne!(answer["result"]["isError"], true, "{answer}");
            assert!(answer.to_string().contains(expected), "{answer}");
        }
    }
}

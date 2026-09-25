//! The MCP tool surface: resolve, docs, api.

use lockdocs_core::query::{Answer, Options, Workspace, DEFAULT_TOKENS};
use serde_json::{json, Value};
use std::path::Path;

pub fn definitions() -> Value {
    let root =
        json!({"type": "string", "description": "Absolute path of the project (defaults to the client's workspace root or the server's working directory)."});
    let tokens = json!({"type": "integer", "description": "Answer budget in tokens (default 1200).", "minimum": 200, "maximum": 20000});
    json!([
        {
            "name": "resolve",
            "title": "Resolve dependency versions",
            "description": "List the exact dependency versions this project pins (from package-lock.json, pnpm-lock.yaml, yarn.lock, bun.lock, Cargo.lock, uv.lock, poetry.lock, requirements*.txt, go.mod) and whether their docs are available locally. Call it to learn which version of a library the code actually uses before writing code against it.",
            "inputSchema": {"type": "object", "properties": {
                "filter": {"type": "string", "description": "Substring of a package name; also searches transitive dependencies."},
                "root": root
            }},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "docs",
            "title": "Search version-exact docs",
            "description": "Answer a question from the docs of the exact installed version of a dependency: README, changelog, docs folder, and API reference (signatures + doc comments from .d.ts, Python, Rust and Go sources). Returns the most relevant sections within a token budget (hybrid keyword + embedding search), each cited as `package@version path:line`. Use it before using a library API you are not sure about for this version. Omit `package` to search all direct dependencies.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string", "description": "What you need, in words or identifiers, e.g. \"strict object unknown keys\" or \"cookies async\"."},
                "package": {"type": "string", "description": "Package name, optionally ecosystem- or version-qualified: zod, npm:zod, pydantic, tokio, github.com/gin-gonic/gin. Comma-separate several."},
                "tokens": tokens,
                "root": root
            }, "required": ["query"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "api",
            "title": "Exact API of a symbol",
            "description": "The exact signature and doc comment of one symbol in the installed version, with overloads, members (for classes, interfaces, structs, traits) and other matches. Accepts `zod.z.object`, `z.object`, `tokio::spawn`, `axum::Router::route`, `pydantic.BaseModel.model_dump`, `gin.Context.JSON`.",
            "inputSchema": {"type": "object", "properties": {
                "symbol": {"type": "string", "description": "Symbol path, optionally prefixed by the package name."},
                "package": {"type": "string", "description": "Package to look in when the symbol has no package prefix."},
                "tokens": tokens,
                "root": root
            }, "required": ["symbol"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        }
    ])
}

pub fn call(ws: &Workspace, opts: &Options, name: &str, args: &Value, root: &Path) -> Result<Answer, String> {
    let s = |k: &str| args.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
    let tokens = args.get("tokens").and_then(|v| v.as_u64()).map_or(DEFAULT_TOKENS, |t| t as usize);
    let engine = ws.engine(root, opts);
    match name {
        "resolve" => Ok(engine.resolve(s("filter"))),
        "docs" => {
            let q = s("query").or_else(|| s("question")).or_else(|| s("topic")).unwrap_or("");
            engine.docs(s("package").or_else(|| s("library")), q, tokens)
        }
        "api" => {
            let sym = s("symbol").or_else(|| s("name")).ok_or("`symbol` is required")?;
            engine.api(sym, s("package"), tokens)
        }
        other => Err(format!("unknown tool `{other}`; tools are resolve, docs, api")),
    }
}

//! The MCP server: lockdocs' tools on mcp-kit (rmcp over stdio).

use crate::{pro, tools};
use lockdocs_core::query::{Options, Workspace};
use mcp_kit::licence::LicencePolicy;
use mcp_kit::rmcp::model::CallToolResult;
use mcp_kit::roots::{self, Sources};
use mcp_kit::server::{run_stdio, App, Call, Info};
use serde_json::Value;
use std::path::PathBuf;

mod calls;

const INSTRUCTIONS: &str = "lockdocs answers from the exact dependency versions pinned in this project's lockfiles, using the installed packages' own docs and type declarations, with anonymous release-tag docs fetched on first use unless offline. Cached docs work offline. Call `resolve` to see versions, `docs` with a question (and optionally `package`) before using an API you are unsure of in this version, and `api` for the exact signature of a symbol like `z.object` or `tokio::spawn`. Every section cites package@version path:line.";

struct Lockdocs {
    ws: Workspace,
    opts: Options,
    default_root: Option<PathBuf>,
    policy: LicencePolicy<'static>,
}

impl Lockdocs {
    fn root(&self, args: &Value, call: &Call) -> Result<PathBuf, String> {
        let explicit = ["root", "repo_root"]
            .iter()
            .find_map(|k| args.get(*k).and_then(|v| v.as_str()))
            .map(PathBuf::from);
        roots::pick(&Sources {
            explicit,
            env: &["LOCKDOCS_ROOT"],
            default: self.default_root.clone(),
            client: &call.client_roots,
        })
    }
}

impl App for Lockdocs {
    fn info(&self) -> Info {
        Info {
            name: "lockdocs".into(),
            title: "lockdocs".into(),
            version: crate::VERSION.into(),
            website: env!("CARGO_PKG_HOMEPAGE").into(),
            instructions: INSTRUCTIONS.into(),
        }
    }

    fn tools(&self) -> Vec<Value> {
        tools::definitions().as_array().cloned().unwrap_or_default()
    }

    fn call(&self, name: &str, args: &Value, call: &Call) -> Result<String, String> {
        self.run(&self.opts, name, args, call).0
    }

    fn call_result(&self, name: &str, args: &Value, call: &Call) -> CallToolResult {
        self.run_licensed(name, args, call)
    }

    /// Read the lockfiles as soon as the client connects, so the first call is fast.
    fn warm(&self, call: &Call) {
        if let Ok(root) = self.root(&Value::Null, call) {
            let _ = self.ws.engine(&root, &self.opts);
        }
    }
}

pub fn serve(default_root: Option<PathBuf>, opts: Options) -> anyhow::Result<()> {
    run_stdio(Lockdocs {
        ws: Workspace::default(),
        opts,
        default_root,
        policy: pro::POLICY,
    })
}

#[cfg(test)]
mod tests;

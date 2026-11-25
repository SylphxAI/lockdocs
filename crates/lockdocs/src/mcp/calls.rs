//! Tool execution, including Pro checks and free upgrade previews.

use super::Lockdocs;
use crate::{preview, pro, tools};
use lockdocs_core::private::PrivateRequired;
use lockdocs_core::query::Options;
use mcp_kit::licence;
use mcp_kit::rmcp::model::{CallToolResult, ContentBlock};
use mcp_kit::server::Call;
use serde_json::Value;

impl Lockdocs {
    /// One tool call with these options; also the packages that needed a private
    /// source the licence did not cover.
    pub(super) fn run(&self, opts: &Options, name: &str, args: &Value, call: &Call) -> (Result<String, String>, Vec<PrivateRequired>) {
        let root = match self.root(args, call) {
            Ok(r) => r,
            Err(e) => return (Err(e), Vec::new()),
        };
        let out = tools::call_checked(&self.ws, opts, name, args, &root);
        let json = args.get("format").and_then(|v| v.as_str()) == Some("json");
        let result = out.result.map(|a| {
            if json {
                serde_json::to_string_pretty(&a.json).unwrap_or_default()
            } else {
                a.text
            }
        });
        (result, out.blocked)
    }

    /// The free answer to an upgrade call: a local preview (counts and a sample),
    /// still carrying `pro_required`. If the preview cannot be computed (target
    /// files not available, unknown package) the reason is appended to the plain
    /// Pro answer.
    fn preview(&self, name: &str, args: &Value, call: &Call, required: &licence::ProRequired) -> CallToolResult {
        let mut result = licence::required_result(required);
        match self
            .root(args, call)
            .and_then(|root| tools::call_checked(&self.ws, &self.opts, name, args, &root).result)
        {
            Ok(answer) => {
                let p = preview::build(&answer.json, required);
                let body = if args.get("format").and_then(|v| v.as_str()) == Some("json") {
                    serde_json::to_string_pretty(&p.json).unwrap_or_default()
                } else {
                    p.text
                };
                result.content = vec![ContentBlock::text(body)];
            }
            Err(e) => result.content = vec![ContentBlock::text(format!("{required}\nPreview unavailable: {e}"))],
        }
        result
    }

    /// Free calls go straight through `call`. The Pro upgrade report (`docs` with
    /// `upgrade_to`) needs a licence; without one the answer is a normal result
    /// with `structuredContent.pro_required`, never an error. So does a call that
    /// needs a private source (a package only a private registry or git host
    /// has): the licence is checked first, and nothing is sent to any private
    /// host without it.
    pub(super) fn run_licensed(&self, name: &str, args: &Value, call: &Call) -> CallToolResult {
        let mut note = None;
        if tools::is_upgrade(name, args) {
            match pro::require(&self.policy, pro::UPGRADE_REPORT) {
                Ok(licence) => note = pro::renewal_note(&self.policy, &licence),
                Err(required) => return self.preview(name, args, call, &required),
            }
        }
        let grant = pro::require(&self.policy, pro::PRIVATE_SOURCES);
        let mut opts = self.opts.clone();
        opts.private = grant.is_ok();
        let (result, blocked) = self.run(&opts, name, args, call);
        if let (Err(required), false) = (&grant, blocked.is_empty()) {
            let named = args.get("package").or_else(|| args.get("library")).is_some();
            if result.is_err() || named {
                return licence::required_result(required);
            }
            let names: Vec<String> = blocked.iter().map(|b| b.package.clone()).collect();
            note = Some(format!(
                "{} need a private source, which is part of lockdocs Pro: {}",
                names.join(", "),
                required.url
            ));
        }
        match result {
            Ok(mut text) => {
                if let Some(n) = note {
                    text.push_str("\n\n");
                    text.push_str(&n);
                }
                CallToolResult::success(vec![ContentBlock::text(text)])
            }
            Err(text) => CallToolResult::error(vec![ContentBlock::text(text)]),
        }
    }
}

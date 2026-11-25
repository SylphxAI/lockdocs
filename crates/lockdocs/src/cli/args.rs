use lockdocs_core::query::{Options, DEFAULT_TOKENS};
use std::collections::HashMap;
use std::path::PathBuf;

pub(super) struct Args {
    pub(super) positional: Vec<String>,
    pub(super) flags: HashMap<String, String>,
}

impl Args {
    pub(super) fn parse(raw: Vec<String>) -> Args {
        let mut positional = Vec::new();
        let mut flags = HashMap::new();
        let takes_value = ["root", "C", "pkg", "package", "tokens", "client", "upgrade-to"];
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
    pub(super) fn flag(&self, k: &str) -> Option<&str> {
        self.flags.get(k).map(|s| s.as_str())
    }
    pub(super) fn take_command(&mut self) -> String {
        if self.on("help") || self.on("h") {
            "help".into()
        } else if self.on("version") || self.on("V") {
            "version".into()
        } else if self.positional.is_empty() {
            // Flags only (`lockdocs --fetch`): the MCP server.
            "mcp".into()
        } else {
            self.positional.remove(0)
        }
    }
    pub(super) fn on(&self, k: &str) -> bool {
        self.flags.get(k).is_some_and(|v| v != "false")
    }
    pub(super) fn root(&self) -> PathBuf {
        self.flag("root")
            .or_else(|| self.flag("C"))
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
    pub(super) fn tokens(&self) -> usize {
        self.flag("tokens").and_then(|t| t.parse().ok()).unwrap_or(DEFAULT_TOKENS)
    }
    pub(super) fn pkg(&self) -> Option<&str> {
        self.flag("pkg").or_else(|| self.flag("package"))
    }
    pub(super) fn opts(&self) -> Options {
        let mut o = Options::default();
        if self.on("fetch") {
            o.fetch = true;
        }
        if self.on("fetch") {
            o.upstream = true;
        }
        if self.on("no-fetch") || self.flag("fetch") == Some("false") || self.on("offline") || std::env::var("LOCKDOCS_OFFLINE").is_ok_and(|v| v != "0") {
            o.fetch = false;
            o.upstream = false;
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::Args;
    #[test]
    fn command_selection_skips_option_values_and_preserves_positionals() {
        for (raw, command, rest) in [
            (vec!["--json", "resolve"], "resolve", vec![]),
            (vec!["--root", "mcp", "docs", "question"], "docs", vec!["question"]),
            (vec!["--root=resolve", "--pkg", "api", "docs", "question"], "docs", vec!["question"]),
            (
                vec!["--offline", "tiny-schema", "reject unknown keys"],
                "tiny-schema",
                vec!["reject unknown keys"],
            ),
            (vec!["--offline", "mcp"], "mcp", vec![]),
            (vec!["--root", "docs", "--offline"], "mcp", vec![]),
            (vec!["--offline", "--help"], "help", vec![]),
            (vec!["--json", "--version"], "version", vec![]),
            (
                vec!["--offline", "--", "tiny-schema", "--literal-question"],
                "tiny-schema",
                vec!["--literal-question"],
            ),
        ] {
            let mut args = Args::parse(raw.into_iter().map(String::from).collect());
            assert_eq!(args.take_command(), command);
            assert_eq!(args.positional, rest);
        }
    }
    #[test]
    fn offline_and_opt_out_override_fetch() {
        for flag in ["--offline", "--no-fetch", "--fetch=false"] {
            let args = Args::parse(vec!["--fetch".into(), flag.into()]);
            let opts = args.opts();
            assert!(!opts.fetch && !opts.upstream);
        }
    }
}

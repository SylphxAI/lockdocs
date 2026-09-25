# CLI

```text
lockdocs <package> [question]   Docs for the version this project pins (no question: overview)
lockdocs resolve [filter]       Pinned versions and where their docs are
lockdocs docs <question>        Search all direct dependencies (--pkg to focus)
lockdocs api <symbol>           Exact signature + doc comment
lockdocs fetch [package...]     Once: upstream docs at each version's git tag, missing packages, the model
lockdocs index [package]        Build indexes ahead of time and show what they hold
lockdocs cache [clean]          Show or delete the cache
lockdocs setup                  Configure MCP clients (--client a,b --dry-run --remove --fetch)
lockdocs mcp                    MCP server on stdio (the default when stdin is not a terminal)
lockdocs version
```

| Option | |
|---|---|
| `-C`, `--root <dir>` | Project directory (default: current) |
| `--pkg <package>` | Package for `docs` / `api` |
| `--tokens <n>` | Answer budget (default 1200) |
| `--fetch` | Allow downloads during queries (missing packages, upstream docs) |
| `--offline` | Never download, not even the embedding model |
| `--json` | Machine-readable output |

Exit code 1 with a message on stderr when a package is not a dependency or not installed.

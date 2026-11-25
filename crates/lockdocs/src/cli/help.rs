pub(super) const HELP: &str = "lockdocs — exact-version library docs for AI agents, straight from your lockfile

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
  upgrade <pkg> <ver>   Pro: API changes from the pinned version to <ver>, limited to what this
                        project calls, with call sites (also: docs --pkg <pkg> --upgrade-to <ver>)
  login [--test] [--no-browser]
                        Sign in and activate a licence (--test requires LOCKDOCS_WEB_URL)
  licence status|activate <token>|buy --pack <id>
                        Show or activate a lockdocs Pro licence, or buy with the device flow
  mcp                   Run the MCP server on stdio (default with no command and non-terminal stdin)
  version               Print the version

Options:
  Global options work before or after a command or package name.
  -C, --root <dir>      Project directory (default: current directory)
  --pkg <package>       Package for docs/api: zod, npm:zod, pydantic@2.9.2, tokio
                        For npm api, an import subpath selects that edition: zod/v4
  --tokens <n>          Answer budget in tokens (default 1200)
  --fetch               Also download missing exact packages and major-version docs sites
                        (cached; also LOCKDOCS_FETCH=1)
  --no-fetch            Disable query downloads (also LOCKDOCS_FETCH=0)
                        Public release-tag docs download on first use by default
  --offline             Never download (not even the embedding model)
  --json                Machine-readable output

Examples:
  lockdocs zod \"strict object unknown keys\"
  lockdocs api axum::Router::route
  lockdocs pydantic \"serialize model to dict\"
  lockdocs resolve

Docs: https://lockdocs.sylphx.com/docs";

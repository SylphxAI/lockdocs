# CLI

```text
lockdocs <package> [question]   Docs for the version this project pins (no question: overview)
lockdocs resolve [filter]       Pinned versions and where their docs are
lockdocs docs <question>        Search all direct dependencies (--pkg to focus)
lockdocs api <symbol>           Exact signature + doc comment
lockdocs fetch [package...]     Once: upstream docs at each version's git tag, missing packages, the model
lockdocs index [package]        Build indexes ahead of time and show what they hold
lockdocs upgrade <pkg> <ver>    Pro: API changes from the pinned version to <ver>, with your call sites
lockdocs licence status|activate <token>   Show or activate a lockdocs Pro licence (free to run)
lockdocs cache [clean]          Show or delete the cache
lockdocs setup                  Configure MCP clients (--client a,b --dry-run --remove --fetch)
lockdocs mcp                    MCP server on stdio (the default when stdin is not a terminal)
lockdocs version
```

| Option | |
|---|---|
| `-C`, `--root <dir>` | Project directory (default: current) |
| `--pkg <package>` | Package for `docs` / `api` |
| `--upgrade-to <version>` | With `docs --pkg`: the Pro upgrade report (same as `lockdocs upgrade`) |
| `--tokens <n>` | Answer budget (default 1200) |
| `--fetch` | Also allow exact registry packages and major-version docs sites during queries |
| `--no-fetch` | Disable query package/docs downloads; cached docs remain usable |
| `--offline` | Never download, not even the embedding model |
| `--json` | Machine-readable output |

Exit code 1 with a message on stderr when a package is not a dependency or not installed. Exit code 3: the command is part of lockdocs Pro and no valid licence is configured (set `LOCKDOCS_LICENCE_TOKEN` or run `lockdocs licence activate <token>`).

Public release-tag docs are fetched anonymously on first `docs` / `api` use by default. `LOCKDOCS_FETCH=0` opts out; `LOCKDOCS_OFFLINE=1` also disables the embedding model download.

### Public npm import entrypoints

`api` follows the installed package's declaration exports from its public import
entrypoint. With `--pkg zod`, it answers for `import … from "zod"`, not whichever
same-named declaration appears elsewhere in the package. Select another public
edition explicitly:

```sh
lockdocs api z.email --pkg zod/v4 --offline
lockdocs api zod/v4.z.email --offline
```

Answers cite the exact package version, declaration file and line, public import
path, and declaration entrypoint/export conditions. A missing export returns
`not_exported`; incomplete applicability returns `unresolved`; conflicting
conditional declarations return `ambiguous`. None is an exact match or silently
substitutes an internal symbol. Alternate public imports are listed separately
with their applicability. JSON answers expose `status`, `import`, `applicability`
and `alternatives`. The MCP `api` tool accepts the same path in `package`.

These queries still work from installed files offline, without an account or a
licence. Dynamic JavaScript exports, external-package re-exports and declaration
syntax that cannot be traced are reported as unresolved rather than guessed.

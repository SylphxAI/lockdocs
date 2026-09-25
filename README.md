<div align="center">

# lockdocs

**Context7 without rate limits: exact-version library docs for AI agents, straight from your lockfile, offline.**

Reads your lockfile. Answers from the docs and type declarations of the exact version you installed.<br>
npm · PyPI · crates.io · Go. One Rust binary. No account, no API key, no rate limit. MIT.

[![npm](https://img.shields.io/npm/v/@sylphx/lockdocs?color=7c9cff&label=npm)](https://www.npmjs.com/package/@sylphx/lockdocs)
[![CI](https://github.com/SylphxAI/lockdocs/actions/workflows/ci.yml/badge.svg)](https://github.com/SylphxAI/lockdocs/actions/workflows/ci.yml)
[![MCP Registry](https://img.shields.io/badge/MCP%20Registry-io.github.SylphxAI%2Flockdocs-42d6a4)](https://registry.modelcontextprotocol.io/)
[![License: MIT](https://img.shields.io/badge/license-MIT-ffb454)](LICENSE)

[Docs](https://sylphxai.github.io/lockdocs/) · [Quickstart](#quickstart) · [Tools](#what-your-agent-gets) · [Benchmarks](#benchmarks) · [Compare](#how-it-compares) · [How it works](#how-it-works)

<img src="docs/public/img/demo.gif" alt="lockdocs demo: the same question about zod in a zod 3 project and a zod 4 project gets .strict() and z.strictObject() respectively, each cited to the installed file and line" width="100%">

<sub>A real terminal: one question, two projects. The zod 3 project gets `.strict()`, the zod 4 project gets `z.strictObject()`, each cited to `package@version file:line`.</sub>

</div>

## Quickstart

```bash
npx -y @sylphx/lockdocs setup     # add lockdocs to Claude Code, Codex, Cursor, VS Code, Claude Desktop, Windsurf, Gemini CLI
```

That's it. `setup` detects the clients you have, writes their MCP config, and prints every change. Run it again and nothing changes. Then ask your agent something like *"Use lockdocs: how do I reject unknown keys with the zod we use?"*

From a terminal, inside any project:

```bash
npx -y @sylphx/lockdocs zod "reject unknown keys"          # docs for the zod version in your lockfile
npx -y @sylphx/lockdocs api axum::Router::route            # exact signature + doc comment
npx -y @sylphx/lockdocs resolve                            # every pinned version, and whether its docs are here
```

<details>
<summary>Manual MCP config</summary>

```json
{
  "mcpServers": {
    "lockdocs": { "command": "npx", "args": ["-y", "@sylphx/lockdocs", "mcp"] }
  }
}
```

Claude Code: `claude mcp add lockdocs -- npx -y @sylphx/lockdocs mcp`
Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.lockdocs]
command = "npx"
args = ["-y", "@sylphx/lockdocs", "mcp"]
```

The server answers for the client's workspace root (or its working directory, or `LOCKDOCS_ROOT`). Every tool also takes `root`.
</details>

## Why

Your agent writes code against the library version it remembers, not the one you installed. So it calls `.dict()` on a pydantic 2 model, `cookies()` without `await` on Next.js 15, and `/:id` routes on axum 0.8. Hosted doc servers help, but they guess the version from the prompt, cap free usage, and cannot see your private packages.

lockdocs takes the version question off the table:

- **Exact version, zero config.** It reads `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`, `Cargo.lock`, `uv.lock`, `poetry.lock`, `Pipfile.lock`, `requirements*.txt` and `go.mod`. No library IDs, no "use v14" in the prompt.
- **Docs that ship with the code.** READMEs, changelogs and `docs/` folders, plus the API reference in the package itself: `.d.ts` declarations with JSDoc, Python docstrings and stubs, rustdoc comments, Go doc comments. If it is installed, it is documented, including your private and internal packages.
- **Offline and unlimited.** Everything is read from `node_modules`, your virtualenv, `~/.cargo/registry` and the Go module cache. No network, no account, no rate limit, and nothing about your dependencies leaves your machine.
- **Small, cited answers.** BM25 over symbols and doc sections, packed into a token budget (2,000 by default), every section cited as `package@version path:line`.

## What your agent gets

Three tools, cheap enough to call before every unfamiliar API:

| Tool | Ask it | Returns |
|---|---|---|
| `resolve` | "Which zod do we use?" | Pinned versions from every lockfile, direct vs transitive, and where their docs are (or what to install) |
| `docs` | "How do I reject unknown keys?" (optionally `package: "zod"`) | The most relevant README, changelog and API sections of the installed version, within budget, each cited `zod@4.1.5 v4/classic/schemas.d.ts:453` |
| `api` | `z.object`, `tokio::spawn`, `BaseModel.model_dump`, `gin.Context.JSON` | The exact signature and doc comment, overloads, members of a class/interface/struct/trait, and other matches |

Example, in a pydantic 2 project:

```text
$ lockdocs api BaseModel.model_dump --tokens 300
pydantic@2.9.2 · pypi · .venv/lib/python3.12/site-packages · pinned in uv.lock

### pydantic.main.BaseModel.model_dump (method) — pydantic@2.9.2 pydantic/main.py:352
def model_dump(self, *, mode: Literal['json', 'python'] | str = 'python', include: IncEx | None = None, …) -> dict[str, Any]
Usage docs: https://docs.pydantic.dev/2.9/concepts/serialization/#modelmodel_dump

Generate a dictionary representation of the model, optionally specifying which fields to include or exclude.
```

The same call in a pydantic 1 project answers that pydantic 1.10.18 has no `model_dump`, and shows the closest documentation instead.

## Ecosystems

| Ecosystem | Versions from | Docs read from | API reference |
|---|---|---|---|
| npm | `package-lock.json`, `npm-shrinkwrap.json`, `pnpm-lock.yaml` (v5-v9), `yarn.lock` (v1 and Berry), `bun.lock`; else `package.json` + `node_modules` | `node_modules` (including pnpm's `.pnpm` store and monorepo roots) | `.d.ts`/`.d.mts`/`.d.cts` with JSDoc; `@types/*` when the package ships none; JSDoc'd JS otherwise |
| PyPI | `uv.lock`, `poetry.lock`, `pdm.lock`, `Pipfile.lock`, `requirements*.txt`; else the virtualenv | `.venv`, `venv`, `$VIRTUAL_ENV`, `$CONDA_PREFIX`, then the system interpreter | Docstrings and signatures from `.py`, `.pyi` stubs; README from the wheel's METADATA |
| crates.io | `Cargo.lock` | `~/.cargo/registry/src` (or `$CARGO_HOME`), `vendor/` | Public items with rustdoc (`///`, `//!`), including items declared inside macros such as tokio's `cfg_rt!` |
| Go | `go.mod` (with `replace`) | `$GOMODCACHE`/`~/go/pkg/mod`, `vendor/` | Exported funcs, methods, types, interface methods with doc comments; package docs |

Legacy copies bundled inside a package (`zod/v3` inside zod 4, `pydantic/v1` inside pydantic 2) rank below the current API.

### Not installed? Fetch the exact version (opt-in)

Out of the box lockdocs never touches the network. If a pinned package is not installed (a fresh clone, CI, a lockfile you are reviewing), it says so and tells you how to install it. Pass `--fetch` (or set `LOCKDOCS_FETCH=1`, or `lockdocs setup --fetch`) to let it download exactly that version from the registry (npm tarball, PyPI wheel or sdist, crates.io `.crate`, Go module proxy zip) into its cache. Fetched answers say `fetched from registry.npmjs.org`. You can also ask for a version you do not use: `lockdocs npm:zod@4.1.5 "strict object" --fetch`.

## Benchmarks

36 questions whose correct answer depends on the version: zod 3 vs 4, Next.js 14 vs 15, React Router 6 vs 7, pydantic 1 vs 2, axum 0.7 vs 0.8, tokio. Each runs in a real project with that version installed; an answer passes when it contains the version-correct API and none of the other version's. Same questions, same grading, against Context7's anonymous tier. Method, questions and scripts are in [`bench/`](bench/), and CI runs it on a GitHub-hosted runner.

See the [benchmark page](https://sylphxai.github.io/lockdocs/benchmarks) for the full per-question table.

## How it compares

| | **lockdocs** | Context7 | docs-mcp-server | Ref |
|---|---|---|---|---|
| Where docs come from | Your installed packages | Hosted index of public repos and sites | Sites and repos you scrape into a local index | Hosted index |
| Which version | Exact, from your lockfile | Chosen by library ID or prompt; a few indexed versions per library | The version you scraped | Mostly latest |
| Per-library setup | None | None | Scrape each library (and version) | None |
| Private / internal packages | Yes, if installed | Paid plans | Yes, if you scrape them | Private GitHub repos and PDFs you connect (plan limits) |
| API reference from types and doc comments | `.d.ts`, docstrings, rustdoc, Go doc | Doc snippets | Doc pages | Doc pages |
| Works offline | Yes | No | After scraping | No |
| Limits | None | Anonymous `ratelimit-limit: 200` (observed); free key 1,000 calls/month | None | 200 free credits once, then from $19/month |
| Account / API key | No | Optional | Optional (embeddings) | Yes |
| License | MIT | MIT client, hosted service | MIT | Proprietary |

Context7 knows many sites and guides that never ship inside a package, so for a question whose answer lives only on a docs website, it can do better. lockdocs is for the other case: when the version matters, when you are offline or rate-limited, and when the package is yours. They combine well.

## How it works

1. **Resolve.** Parse every lockfile in the project (and monorepo roots above it) into exact `(ecosystem, name, version)` triples, marking direct dependencies.
2. **Locate.** Find each package's files on disk and check the installed version against the lockfile. Drift is reported, never hidden.
3. **Extract.** Split READMEs, changelogs and doc folders into heading-scoped sections; parse `.d.ts`, Python, Rust and Go sources with tree-sitter into symbols with signatures, doc comments and qualified paths (`z.object`, `tokio::task::spawn`, `pydantic.main.BaseModel.model_dump`).
4. **Index and cache.** BM25 (identifier-aware tokenizer, light stemming, a small programming synonym table) per package, cached on disk by package, version and source, so each version is indexed once, ever.
5. **Answer.** Rank, then pack results into the token budget with citations.

Typical costs: indexing zod 4 (3,000+ symbols) takes about 0.15 s once; a warm query takes tens of milliseconds.

## CLI

```text
lockdocs <package> [question]   Docs for the version this project pins (no question: overview)
lockdocs resolve [filter]       Pinned versions and where their docs are
lockdocs docs <question>        Search all direct dependencies (--pkg to focus)
lockdocs api <symbol>           Exact signature + doc comment
lockdocs index [package]        Build indexes ahead of time
lockdocs cache [clean]          Show or delete the cache
lockdocs setup                  Configure MCP clients (--client a,b --dry-run --remove --fetch)
lockdocs mcp                    MCP server on stdio

Options: -C/--root <dir>, --pkg <package>, --tokens <n>, --fetch, --json
```

Prebuilt binaries for macOS (arm64, x64), Linux glibc (x64, arm64) and Windows x64 ship through npm; each [GitHub release](https://github.com/SylphxAI/lockdocs/releases) has them too. From source: `cargo install --git https://github.com/SylphxAI/lockdocs lockdocs`.

## Privacy

lockdocs reads files on your machine and answers over stdio. It makes no network calls unless you enable fetching, and then only to the public registries named above, for the exact package versions requested. The cache lives in your OS cache directory (`LOCKDOCS_CACHE` overrides it).

## Also from Sylphx

- [**repomap**](https://github.com/SylphxAI/repomap): a map of your codebase for AI agents: code graph, search, call paths and change impact, with an interactive graph UI.
- [**anymd**](https://github.com/SylphxAI/anymd): any file to clean Markdown for your AI agent: PDF, Word, PowerPoint, Excel, EPUB, HTML, images, audio and video.

All three run locally, need no API key, and are MIT licensed.

## Star history

[![Star History Chart](https://api.star-history.com/svg?repos=SylphxAI/lockdocs&type=Date)](https://star-history.com/#SylphxAI/lockdocs&Date)

## License

MIT © Sylphx

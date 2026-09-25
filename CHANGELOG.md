# Changelog

## 0.3.0

- **Docs sites follow your major.** For packages whose docs live in a separate website repository, `lockdocs fetch` takes the docs for the pinned major: the default branch when your major is the latest, a `vN` / `N.x` branch when the site keeps one (Tailwind CSS v3, Prisma v6), or the last commit before the next major was released. Pages about a later major are skipped. React keeps the latest-only rule (react.dev documents APIs before they ship). tokio's website (tutorial and topics) is added, and docs sites now work for crates and PyPI packages too.
- **More of the docs are read.** Django's docs (reStructuredText in `.txt` files) were downloaded but not indexed; they are now. Docs pages written as React components (Tailwind's installation guides) are indexed. HTML headings in MDX split sections, and `export const title` names the page.
- **Ranking.** A second BM25 over just the heading (or name) and first sentence keeps a long body from burying what an entry says it is. Question words that name a documented top-level API of the package ("run code *after* the response" in Next.js, which exports `after`) count as identifiers.
- **Fetch.** GitHub API redirects (renamed repositories) keep the token, docs-site errors appear in the fetch note, and `lockdocs fetch` refreshes copies made by older versions.
- **Benchmark:** 17 held-out questions (not used for tuning) in their own column; Context7 answers are reused only when the question and its grading are unchanged; the `pyd1-config` grader no longer rejects `ConfigDict`, which pydantic 1.10 also ships.

## 0.2.1

- **MCP server** now runs on [mcp-kit](https://github.com/SylphxAI/mcp-kit), which uses rmcp, the official Rust MCP SDK, instead of lockdocs' own JSON-RPC loop. Tools and answers are unchanged. The server now also handles protocol negotiation across every spec version, cancellation, progress and pagination.
- **Shared parts:** `setup`, the npm launcher and the release workflow now come from mcp-kit, shared with the other Sylphx MCP servers.

## 0.2.0

- **Hybrid retrieval.** BM25 is fused with dense similarity from a small local embedding model (model2vec `potion-retrieval-32M`, distilled from bge-base-en-v1.5, MIT). It is downloaded once (129 MB, SHA-256 verified, stored int8-quantized at 32 MB) on first use; `LOCKDOCS_EMBED=0` or `--offline` keeps lockdocs keyword-only, and any failure falls back to BM25.
- **API redirects.** Deprecation notes ("use `model_validate` instead", "Consider `z.strictObject`") lift the API they point to; doc-comment summaries weigh like headings; name matches count only for words that are rare in the package.
- **Upstream docs at the exact tag.** `lockdocs fetch` downloads, once, the docs folders of each dependency's GitHub repository at the git tag of the pinned version (repository from the package metadata; MDX front matter and includes handled; docs example files inlined), plus missing packages and the embedding model. With `--fetch` / `LOCKDOCS_FETCH=1` / `setup --fetch` this happens automatically on first query. Answers from packages that ship few docs suggest it.
- **Yarn Plug'n'Play** zip caches (project `.yarn/cache` and the global Berry cache) and **Cargo git dependencies** (`~/.cargo/git/checkouts`).
- Default answer budget 1,200 tokens (was 2,000), with a relevance floor that stops at weak matches.
- Benchmark: 70 questions over 14 libraries (added Tailwind, ESLint, Prisma, React, Vite, Express, SQLAlchemy, Django, FastAPI, Next.js 16), three lockdocs configurations against Context7.

## 0.1.0

First release.

- Exact versions from `package-lock.json`, `npm-shrinkwrap.json`, `pnpm-lock.yaml` (v5-v9), `yarn.lock` (v1, Berry), `bun.lock`, `Cargo.lock`, `uv.lock`, `poetry.lock`, `pdm.lock`, `Pipfile.lock`, `requirements*.txt` and `go.mod`, with fallbacks to `node_modules` and the virtualenv.
- Docs from installed packages: READMEs, changelogs, docs folders, and API reference from `.d.ts` + JSDoc (and `@types/*`), Python docstrings and stubs, rustdoc (including items inside `cfg_*!` macros) and Go doc comments.
- BM25 with an identifier-aware tokenizer, stemming and programming synonyms; token-budgeted answers cited `package@version path:line`; per-version on-disk index cache.
- MCP tools `resolve`, `docs`, `api`; CLI parity (`lockdocs <package> "<question>"`); `lockdocs setup` for Claude Code, Codex, Cursor, VS Code, Claude Desktop, Windsurf and Gemini CLI.
- Opt-in `--fetch` of exact versions from npm, PyPI, crates.io and the Go proxy.
- Benchmark of 36 version-sensitive questions against real installs, with Context7 for comparison.

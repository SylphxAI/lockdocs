# Changelog

## 0.1.0

First release.

- Exact versions from `package-lock.json`, `npm-shrinkwrap.json`, `pnpm-lock.yaml` (v5-v9), `yarn.lock` (v1, Berry), `bun.lock`, `Cargo.lock`, `uv.lock`, `poetry.lock`, `pdm.lock`, `Pipfile.lock`, `requirements*.txt` and `go.mod`, with fallbacks to `node_modules` and the virtualenv.
- Docs from installed packages: READMEs, changelogs, docs folders, and API reference from `.d.ts` + JSDoc (and `@types/*`), Python docstrings and stubs, rustdoc (including items inside `cfg_*!` macros) and Go doc comments.
- BM25 with an identifier-aware tokenizer, stemming and programming synonyms; token-budgeted answers cited `package@version path:line`; per-version on-disk index cache.
- MCP tools `resolve`, `docs`, `api`; CLI parity (`lockdocs <package> "<question>"`); `lockdocs setup` for Claude Code, Codex, Cursor, VS Code, Claude Desktop, Windsurf and Gemini CLI.
- Opt-in `--fetch` of exact versions from npm, PyPI, crates.io and the Go proxy.
- Benchmark of 36 version-sensitive questions against real installs, with Context7 for comparison.

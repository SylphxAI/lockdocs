# lockdocs

Exact-version library docs for AI agents, from the project's lockfile, offline.
It ships as a Rust MCP server and CLI, runs locally, needs no API key, and is
MIT licensed.

- Lifecycle: `active`, published as `@sylphx/lockdocs` (npm) and
  `io.github.SylphxAI/lockdocs` (MCP Registry)
- Docs: https://sylphxai.github.io/lockdocs/

## Layout

- `crates/lockdocs-core`: lockfile parsers (`lockfile.rs`), project loading
  (`project.rs`), finding installed sources (`locate.rs`), opt-in registry
  fetch (`fetch.rs`), Markdown sections (`markdown.rs`), tree-sitter symbol
  extraction for TypeScript, Python, Rust and Go (`extract.rs`), BM25
  (`bm25.rs`), the per-version index cache (`index.rs`), and the three queries
  (`query.rs`)
- `crates/lockdocs`: the `lockdocs` binary (CLI, MCP stdio server, `setup`)
- `packages/lockdocs`: the npm launcher; `packages/npm/*`: native binaries
- `bench/`: version-sensitive questions, project pins, and the runner
- `docs/`: the VitePress site; `scripts/`: version sync

## Release

Bump with `bun scripts/set-version.ts X.Y.Z && cargo update -w`, add a
`## X.Y.Z` section to CHANGELOG.md, then merge. `release.yml` publishes when
the npm version is new: 5 native targets on GitHub-hosted runners, the natives
and `@sylphx/lockdocs`, an `npx` smoke on a real project, the GitHub release,
and the MCP Registry entry.

npm publishing uses trusted publishing (OIDC): every package
(`@sylphx/lockdocs` and the five `@sylphx/lockdocs-*` natives) trusts
`SylphxAI/lockdocs` `.github/workflows/release.yml`; there is no npm token.

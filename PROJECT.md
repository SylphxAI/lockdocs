# lockdocs

Exact-version library docs for AI agents, from the project's lockfile, offline.
It ships as a Rust MCP server and CLI, runs locally, needs no API key, and is
MIT licensed.

- Published as `@sylphx/lockdocs` (npm) and
  `io.github.SylphxAI/lockdocs` (MCP Registry)
- Docs: https://lockdocs.sylphx.com/docs

## Layout

- `crates/lockdocs-core`: lockfile parsers (`lockfile.rs`), project loading
  (`project.rs`), finding installed sources (`locate.rs`), opt-in registry
  fetch (`fetch.rs`), Markdown sections (`markdown.rs`), tree-sitter symbol
  extraction for TypeScript, Python, Rust and Go (`extract.rs`), BM25
  (`bm25.rs`), the per-version index cache (`index.rs`), and the three queries
  (`query.rs`), and embedding policy (`semantic.rs`, with the engine and
  tokenizer from `sylphx-mcp-kit` 0.3)
- `crates/lockdocs`: the `lockdocs` binary (CLI, MCP stdio server, `setup`)
- `packages/lockdocs`: the npm launcher; `packages/npm/*`: native binaries
- `bench/`: version-sensitive questions, project pins, and the runner
- `docs/`: the VitePress site; `scripts/`: version sync
- `brand/`: the brand home — vector masters, generated favicons and app icons,
  colour and type tokens. The site's logo, favicon and theme colours are copies
  of it. See `brand/README.md`

## Release

Bump with `bun scripts/set-version.ts X.Y.Z && cargo update -w`, add a
`## X.Y.Z` section to CHANGELOG.md, then merge. `release.yml` publishes when
the npm version is new: 5 native targets on GitHub-hosted runners, the natives
and `@sylphx/lockdocs`, an `npx` smoke on a real project, the GitHub release,
and the MCP Registry entry.

The caller-owned `package-managers` job then updates Homebrew and Scoop from
those release archives, including on a no-op release run. It exchanges its
GitHub OIDC identity `lockdocs-package-managers` through Access and the Broker
for separate repository-scoped tokens. Missing authorization fails the job;
the company App key stays in the Broker.

npm publishing uses trusted publishing (OIDC): every package
(`@sylphx/lockdocs` and the five `@sylphx/lockdocs-*` natives) trusts
`SylphxAI/lockdocs` `.github/workflows/release.yml`; there is no npm token.

### Official MCP Registry

The [Registry listing](https://registry.modelcontextprotocol.io/v0.1/servers/io.github.SylphxAI%2Flockdocs/versions/latest)
uses `io.github.SylphxAI/lockdocs`, with the same capitalization as the published
npm package's `mcpName`. Keep that name in `server.json` and in the release
workflow so existing installations stay on the same listing.

The release workflow normally publishes the Registry metadata after npm. To
check the manifest without publishing, install the official
[mcp-publisher](https://github.com/modelcontextprotocol/registry/releases) and
run these from the repository root:

```bash
bun scripts/check-mcp-registry.ts
mcp-publisher validate server.json
```

For manual recovery only, wait for the npm release and make sure no release run
is publishing the same version. Sign in with `mcp-publisher login github` as
an authorized maintainer (the release workflow uses `github-oidc`). Guard
the publish with the preflight so it stops if npm metadata doesn't match, the
version already exists, or the Registry can't be checked:

```bash
mcp-publisher validate server.json &&
  bun scripts/check-mcp-registry.ts --preflight &&
  mcp-publisher publish server.json
```

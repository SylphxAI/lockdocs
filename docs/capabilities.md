# lockdocs capabilities

What lockdocs can do today and where the code is. CI checks that every path
in the Code column exists (`scripts/check-capabilities.ts`). The destination
is in [vision.md](vision.md).

| ID | Capability | Status | Code | Depends on |
| --- | --- | --- | --- | --- |
| LD-LOCKFILE | Read exact versions from npm, pnpm, Yarn, Bun, Cargo, uv, Poetry, PDM, Pipenv, pip requirements and Go lockfiles | supported | crates/lockdocs-core/src/lockfile.rs, crates/lockdocs-core/src/project.rs | |
| LD-LOCATE | Find each package's files on disk (node_modules, Yarn Plug'n'Play, virtualenvs, Cargo registry and git checkouts, Go module cache) | supported | crates/lockdocs-core/src/locate.rs | LD-LOCKFILE |
| LD-REGISTRY-FETCH | Download a pinned package that is not installed, from its registry (opt-in) | supported | crates/lockdocs-core/src/fetch.rs | LD-LOCKFILE |
| LD-UPSTREAM | Download the docs folders of a package's GitHub repository at the immutable commit resolved from the pinned release tag (automatic on first query; opt-out/offline supported) | supported | crates/lockdocs-core/src/upstream.rs | LD-LOCATE |
| LD-DOCS-SITE | Add an official docs-site repository for the pinned major: the default branch for the latest major, a `vN` branch or the last commit before the next major for older ones (React, Express, Tailwind CSS, Prisma, tokio) | partial | crates/lockdocs-core/src/upstream.rs | LD-UPSTREAM |
| LD-EXTRACT | Split Markdown, MDX, reStructuredText and component-based docs pages into sections; extract TypeScript, Python, Rust and Go symbols with signatures and doc comments | supported | crates/lockdocs-core/src/extract.rs, crates/lockdocs-core/src/markdown.rs | LD-LOCATE |
| LD-INDEX | Cache one index per package, version and source location | supported | crates/lockdocs-core/src/index.rs, crates/lockdocs-core/src/cache.rs | LD-EXTRACT |
| LD-SEARCH | Rank sections and symbols: BM25 on full text and on headings, a local embedding model, and docs-specific signals | supported | crates/lockdocs-core/src/bm25.rs, crates/lockdocs-core/src/semantic.rs, crates/lockdocs-core/src/query.rs | LD-INDEX |
| LD-MCP | MCP server with the `resolve`, `docs` and `api` tools over stdio | supported | crates/lockdocs/src/mcp.rs, crates/lockdocs/src/tools.rs | LD-SEARCH |
| LD-CLI | Command line with the same queries, plus `index`, `fetch` and `setup` | supported | crates/lockdocs/src/main.rs, crates/lockdocs/src/setup.rs | LD-SEARCH |
| LD-NPM | npm launcher and native binaries for five platforms | supported | packages/lockdocs, packages/npm | LD-CLI |
| LD-BENCH | Version-sensitive benchmark against Context7, run on GitHub-hosted runners | supported | bench/run.py, bench/questions.json, .github/workflows/bench.yml | LD-CLI |
| LD-PRO-LICENCE | lockdocs Pro licence: offline token check, `licence status/activate`, the `pro_required` answer | supported | crates/lockdocs/src/pro.rs | |
| LD-PRO-UPGRADE | Pro: upgrade report, an API diff from the pinned version to a target, limited to the symbols the project calls, with call sites | supported | crates/lockdocs-core/src/upgrade.rs | LD-REGISTRY-FETCH, LD-EXTRACT, LD-PRO-LICENCE |
| LD-PRO-PRIVATE | Pro: read packages and upstream docs from the private registries and git hosts the project already configures (`.npmrc`, uv and pip config, `.cargo/config.toml`, `GOPROXY`/`GOPRIVATE`, `~/.netrc`, token variables, `git credential fill`), each credential sent only to its own host | supported | crates/lockdocs-core/src/private, crates/lockdocs-core/src/fetch.rs, crates/lockdocs-core/src/upstream.rs, crates/lockdocs/src/pro.rs | LD-REGISTRY-FETCH, LD-UPSTREAM, LD-PRO-LICENCE |

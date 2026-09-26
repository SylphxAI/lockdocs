# lockdocs capabilities

What lockdocs can do today and where the code is. CI checks that every path
in the Code column exists (`scripts/check-capabilities.ts`). The destination
is in [vision.md](vision.md).

| ID | Capability | Status | Code | Depends on |
| --- | --- | --- | --- | --- |
| LD-LOCKFILE | Read exact versions from npm, pnpm, Yarn, Bun, Cargo, uv, Poetry, PDM, Pipenv, pip requirements and Go lockfiles | supported | crates/lockdocs-core/src/lockfile.rs, crates/lockdocs-core/src/project.rs | |
| LD-LOCATE | Find each package's files on disk (node_modules, Yarn Plug'n'Play, virtualenvs, Cargo registry and git checkouts, Go module cache) | supported | crates/lockdocs-core/src/locate.rs | LD-LOCKFILE |
| LD-REGISTRY-FETCH | Download a pinned package that is not installed, from its registry (opt-in) | supported | crates/lockdocs-core/src/fetch.rs | LD-LOCKFILE |
| LD-UPSTREAM | Download the docs folders of a package's GitHub repository at the pinned version's git tag (opt-in) | supported | crates/lockdocs-core/src/upstream.rs | LD-LOCATE |
| LD-DOCS-SITE | Add an official docs-site repository for the pinned major: the default branch for the latest major, a `vN` branch or the last commit before the next major for older ones (React, Express, Tailwind CSS, Prisma, tokio) | partial | crates/lockdocs-core/src/upstream.rs | LD-UPSTREAM |
| LD-EXTRACT | Split Markdown, MDX, reStructuredText and component-based docs pages into sections; extract TypeScript, Python, Rust and Go symbols with signatures and doc comments | supported | crates/lockdocs-core/src/extract.rs, crates/lockdocs-core/src/markdown.rs | LD-LOCATE |
| LD-INDEX | Cache one index per package, version and source location | supported | crates/lockdocs-core/src/index.rs, crates/lockdocs-core/src/cache.rs | LD-EXTRACT |
| LD-SEARCH | Rank sections and symbols: BM25 on full text and on headings, a local embedding model, and docs-specific signals | supported | crates/lockdocs-core/src/bm25.rs, crates/lockdocs-core/src/embed.rs, crates/lockdocs-core/src/query.rs | LD-INDEX |
| LD-MCP | MCP server with the `resolve`, `docs` and `api` tools over stdio | supported | crates/lockdocs/src/mcp.rs, crates/lockdocs/src/tools.rs | LD-SEARCH |
| LD-CLI | Command line with the same queries, plus `index`, `fetch` and `setup` | supported | crates/lockdocs/src/main.rs, crates/lockdocs/src/setup.rs | LD-SEARCH |
| LD-NPM | npm launcher and native binaries for five platforms | supported | packages/lockdocs, packages/npm | LD-CLI |
| LD-BENCH | Version-sensitive benchmark against Context7, run on our own Linux CI runners | supported | bench/run.py, bench/questions.json, .github/workflows/bench.yml | LD-CLI |
| LD-SEMANTIC-RERANK | Rerank the top results with a stronger local model for questions worded differently from the docs | planned | | LD-SEARCH |

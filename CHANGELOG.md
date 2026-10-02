# Changelog

## Unreleased

- **First-use fetch no longer depends on GitHub's REST API quota.** Upstream docs are found with `git ls-remote`-style tag lookups and read from one streamed `codeload.github.com` tarball per repository, filtered to docs paths, instead of the REST tree and per-file requests (60 per hour per IP anonymously; 6 of 10 benchmark projects failed behind a shared IP). Cold-cache, no token: 10/10 on the 10-question version set, no 403, 1-5 GitHub connections per project instead of 2-28. `upstream::FORMAT` is 5. A missing tag error lists the tag names tried. An archive over 150 MB compressed (or a codeload failure) falls back to the per-file REST path for that repository only, and the manifest records `via: codeload` or `rest`. Measured (cold cache, no token, 1 s pacing between GitHub connections, same 10-question set): before 8/10 correct, 2 projects hit 403, 152 GitHub connections (34 REST, 118 raw), 145 s total fetch time, 1.8 MB down; after 10/10, 0 403s, 31 connections (17 codeload, none REST or raw), 27 s, 56 MB down. Per package, next 14.2.35 and 15.1.0 download a 42-46 MB tarball in 2.7-5 s and TypeScript 5.6.3 a 32 MB one in 8 s, each over 2 connections, all through codeload. Trade-off: bytes go up (the tarball holds the whole repository) while connections and quota use go down.

## 0.4.0

- **Useful docs on first use.** `docs` and `api` now automatically add public upstream docs for the requested release, anonymously. The existing fetcher resolves exact `refs/tags/` refs, rejects same-named branches, and peels annotated tags to an immutable commit before downloading bounded docs. Missing registry packages and major-version website docs remain explicit opt-in.
- **Clear network controls.** `--no-fetch` or `LOCKDOCS_FETCH=0` disables query package/docs downloads; `--offline` or `LOCKDOCS_OFFLINE=1` prevents all downloads, including the embedding model. MCP `offline: true` restricts a call to installed/cached package and upstream files. Automatic upstream requests never use ambient GitHub credentials.
- **Honest provenance and failures.** Structured answers include the source, requested version and upstream manifest; ordinary broad text answers retain a budgeted source/fallback summary. Fetch reports preserve their original flat fields alongside richer provenance. Installed-version drift is rejected instead of substituted, and git dependencies use checkout files rather than registry releases or guessed tags.
- **Safe cache enrichment and retry.** Explicit fetching upgrades release-only caches with docs-site enrichment. Later default/offline queries preserve opted-in site docs and provenance. Transient registry lookup or file-download failures do not become permanent empty-doc results or overwrite a compatible complete cache; failures are disclosed and explicit enrichment remains retryable. Legacy format-3 caches are revalidated online and disclosed when read offline.
- **Shared runtime.** Embeddings, identifier tokenization and supported cache-root resolution now use mcp-kit 0.3, alongside the shared MCP server, setup and npm launcher.
- **One-click editor setup.** README and docs offer Cursor and VS Code install links; desktop bundles prompt for the project folder used to resolve the lockfile.
- **Measured defaults.** The benchmark separately measures real first-use behavior from an empty isolated cache without credentials: 70/105. Explicit prefetch remains 96/105 and package-only hybrid 60/105; the prefetch score is not presented as the default score.
- The CLI prints one GitHub star line to stderr after the fifth successful interactive run, once ever (counter in the cache directory). It is silent for the MCP server, with `--json`, in CI, and when stderr is not a terminal; `LOCKDOCS_NO_STAR_HINT=1` turns it off.

## 0.3.0

- **Docs sites follow your major.** For packages whose docs live in a separate website repository, `lockdocs fetch` takes the docs for the pinned major: the default branch when your major is the latest, a `vN` / `N.x` branch when the site keeps one (Tailwind CSS v3, Prisma v6), or the last commit before the next major was released. Pages about a later major are skipped. React keeps the latest-only rule (react.dev documents APIs before they ship). tokio's website (tutorial and topics) is added, and docs sites now work for crates and PyPI packages too.
- **More of the docs are read.** Django's docs (reStructuredText in `.txt` files) were downloaded but not indexed; they are now. Docs pages written as React components (Tailwind's installation guides) are indexed. HTML headings in MDX split sections, and `export const title` names the page.
- **Ranking.** A second BM25 over just the heading (or name) and first sentence keeps a long body from burying what an entry says it is. Question words that name a documented top-level API of the package ("run code *after* the response" in Next.js, which exports `after`) count as identifiers. Generic headings (Parameters, Returns, Examples) take their topic from the heading above; MDX heading ids (`{/*usage*/}`) are dropped; capitalized words in headings stay whole (TypeScript no longer matches "type"). Upgrade guides to an older major than yours and pages titled "(Deprecated)" rank lower unless the question is about changes. Code-only sections are embedded with their code, not their title alone. The stemmer pairs -ation/-ate and -ability/-able, and the synonym table adds parameter/param, JavaScript/JS, TypeScript/TS and database/DB.
- **Answers.** The top result quotes the first paragraph of its page and parent section, with the short list or code block that follows, when they add something (React's "In React 19, forwardRef is no longer necessary" at the top of the page; the `@custom-variant` setup a Tailwind subsection builds on).
- **Fetch.** GitHub API redirects (renamed repositories) keep the token, docs-site errors appear in the fetch note, and `lockdocs fetch` refreshes copies made by older versions.
- **Benchmark:** 105 questions (was 70). 17 questions written as held-out were used to diagnose misses after their first run and joined the main set; 18 new held-out questions, not used for tuning, have their own column. Context7 answers are reused only when the question and its grading are unchanged, and a Context7 library that answers HTTP 404 falls back to the next search result. The pydantic 1 graders reject the v2 idiom `model_config = ConfigDict` instead of `ConfigDict`, which pydantic 1.10 also ships. `bench.yml` takes a `variants` input for weight sweeps.

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

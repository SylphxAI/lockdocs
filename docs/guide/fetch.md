# Fetching and upstream docs

lockdocs starts with installed package files and automatically adds public release-tag docs on first `docs` or `api` use. Downloads are cached under the supported OS cache root (`LOCKDOCS_CACHE` overrides it).

## First-use defaults and controls

The query's selected packages are enriched, not every dependency in the project. The existing upstream fetcher reads the repository from package metadata, resolves a candidate tag for exactly the requested version to a full immutable commit, then reads only that commit's docs. Automatic fetches are anonymous: ambient `GITHUB_TOKEN` and `GH_TOKEN` are not read. They do not use major-version docs sites or fall back to a default branch/latest version.

- `--no-fetch` or `LOCKDOCS_FETCH=0`: no query package/upstream downloads; installed and cached docs still work. The model has its own policy.
- `--offline` or `LOCKDOCS_OFFLINE=1`: no downloads, including the embedding model. `fetch --offline` is rejected.
- MCP `docs` / `api` with `offline: true`: no package/upstream downloads for that call. Use server `--offline` to disable the background model download too.
- `LOCKDOCS_NO_UPSTREAM=1`: package-only answers, even if upstream docs are cached.
- `--fetch` or `LOCKDOCS_FETCH=1`: explicitly enable missing registry packages and major-version docs-site enrichment too.

Automatic upstream work has a 45-second scheduling budget per package and a 10-second timeout per request (an in-flight request may finish after the scheduling deadline), no redirects, at most 2,500 files / 40 MB of advertised content and a 2 MB per-file limit, with 16 downloads in flight. Failed or missing enrichment is reported in text and structured `provenance`; answers retain the requested version's package files. Successful caches include repository, release tag, immutable commit, file counts and partial-download notes. Later queries reuse them without network. A failed fetch preserves the previous complete cache; concurrent fetches fail without waiting. An interrupted process can leave a fetch lock; `lockdocs cache clean` removes it.

Network requests reveal the public repository/version/file being fetched, not the question or project contents. Anonymous GitHub limits can stop a cold multi-package session; failures are reported, not retried within an indexed MCP session.

## The embedding model (automatic, once)

On the first query, lockdocs downloads the embedding model it uses next to BM25 (model2vec `potion-retrieval-32M`, 129 MB, MIT) from huggingface.co at a pinned revision, checks its SHA-256, and stores it int8-quantized (32 MB) in the cache. It prints one line when it does. The MCP server downloads it in the background and answers keyword-only until it lands. `LOCKDOCS_EMBED=0` or `--offline` skips it; any failure falls back to keyword search.

## `lockdocs fetch`: upstream docs at the exact tag

Many packages ship no documentation: Next.js, Django, FastAPI, React Router's guides and zod 4's docs live only in their repositories. `lockdocs fetch` finds each direct dependency's GitHub repository from its own metadata (`package.json` `repository`, PyPI `Project-URL`, `Cargo.toml` `repository`, the Go module path), finds the git tag of the pinned version (`v1.2.3`, `1.2.3`, `name@1.2.3`, `name-v1.2.3`, `rel_1_2_3`, ...), and downloads only the docs folders at that tag (Markdown, MDX, reStructuredText and docs example files):

```bash
lockdocs fetch              # every direct dependency
lockdocs fetch next zod     # just these
```

```text
Fetched for 3 packages in 10016 ms (cached; later queries stay offline):
  model   potion-retrieval-32M ready
  next@15.1.0      upstream docs github.com/vercel/next.js@v15.1.0: 365 files (2.0 MB)
```

Answers then cite `next@15.1.0 upstream:docs/01-app/.../cookies.mdx:12` and name the repository and tag in the header.

Some projects keep their docs in a separate website repository: React (react.dev), Express (expressjs.com), Tailwind CSS (tailwindcss.com), Prisma (prisma/docs) and tokio (tokio-rs/website). For these, `fetch` also takes the docs that describe your major:

- your major is the latest: the site's default branch;
- an older major: the site's `vN` or `N.x` branch when it has one (Tailwind CSS `v3`, Prisma `v6`), otherwise the last commit before the next major was released (the date of its `N+1.0.0` tag). React is excluded from the second rule because react.dev documents APIs before they ship;
- pages about a later major (`v4-beta.mdx` on the v3 branch) are skipped.

The header says which: `github.com/prisma/docs@v6 (branch v6 for major 6)`. After upgrading lockdocs, run `lockdocs fetch` again to refresh copies made by an older version. For explicit `fetch` / `--fetch` only, set `GITHUB_TOKEN` (or `GH_TOKEN`) to raise GitHub's API limit (60 requests an hour without it; a package takes 2 to 20).

Release-tag enrichment already happens on first query. Separate website repositories above describe a major, not an exact patch version, and remain explicit-fetch-only; their provenance is included in the answer header.

## Packages that are not installed

With fetching enabled, a pinned package that is not on disk is downloaded at exactly that version (npm tarball, PyPI wheel or sdist, crates.io `.crate`, Go module zip), and you can ask about a version you do not use:

```bash
lockdocs npm:zod@4.1.5 "reject unknown keys" --fetch
```

Only docs and source files are unpacked (no binaries, nothing outside the target directory, 64 MB download cap). Git dependencies are never fetched from a registry.

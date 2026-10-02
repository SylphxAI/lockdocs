# Fetching and upstream docs

lockdocs starts with installed package files and automatically adds public release-tag docs on first `docs` or `api` use. Downloads are cached under the supported OS cache root (`LOCKDOCS_CACHE` overrides it).

## First-use defaults and controls

The query's selected packages are enriched, not every dependency in the project. The existing upstream fetcher reads the repository from package metadata, lists the exact `refs/tags/` refs for the requested version's tag names over git's smart-HTTP protocol (what `git ls-remote` does; the server peels annotated tags to a full immutable commit, and a same-named branch is rejected), then streams that commit's `codeload.github.com` tarball and keeps only its docs. If the archive is over the cap (150 MB for explicit fetch, 64 MB for the automatic first-use fetch), codeload fails, or too little of the automatic time budget is left, that repository alone is read file by file through the REST API instead, and the manifest records `via: rest`. Neither step of the main path uses the GitHub REST API, so there is no 60-requests-an-hour limit and no token is needed (and none is sent to those hosts, for automatic or explicit fetch). A tag that does not exist is reported with the tag names tried. Automatic fetches are anonymous: ambient `GITHUB_TOKEN` and `GH_TOKEN` are not read. Git dependencies use their checkout files rather than guessing a release tag for their resolved commit. Automatic fetches do not download major-version docs sites or fall back to a default branch/latest version.

- `--no-fetch` or `LOCKDOCS_FETCH=0`: no query package/upstream downloads; installed and cached docs still work. The model has its own policy.
- `--offline` or `LOCKDOCS_OFFLINE=1`: no downloads, including the embedding model. `fetch --offline` is rejected.
- MCP `docs` / `api` with `offline: true`: no package/upstream downloads for that call. Use server `--offline` to disable the background model download too.
- `LOCKDOCS_NO_UPSTREAM=1`: package-only answers, even if upstream docs are cached.
- `--fetch` or `LOCKDOCS_FETCH=1`: explicitly enable missing registry packages and major-version docs-site enrichment too.

Automatic upstream work has a 45-second scheduling budget per package and a 10-second timeout per request (an in-flight request may finish after the scheduling deadline), no redirects, at most 2,500 files / 40 MB of advertised content and a 2 MB per-file limit, with 16 downloads in flight. Failed or missing enrichment is reported in text and structured `provenance`; answers retain the requested version's package files. Successful caches include repository, release tag, immutable commit, file counts and whether explicit docs-site enrichment completed. Later queries reuse them without network. Any failed file or docs-site lookup aborts publication: a compatible previous complete cache is retained and returned with a transient failure note. Without a compatible prior cache, the failure is reported and no permanent negative cache is written. Concurrent fetches fail without waiting. An interrupted process can leave a fetch lock; `lockdocs cache clean` removes it.

A release-only automatic cache is upgraded when explicit enrichment is requested. Once docs sites have been explicitly cached, default and offline queries preserve and reuse those files and their provenance without destructive refresh. Transient registry HTTP, network, body-read or parsing failures leave docs-site enrichment incomplete and retryable; only a successful lookup (including a genuine missing registry record) can mark it checked. Truly empty selections are cached separately from failed downloads. Legacy format-3 caches are revalidated online because their commit candidates could have named branches. Offline use can retain those files, with an explicit unverified-cache fallback note. Broad text answers include a bounded fallback/source summary; JSON carries complete provenance once at the top level.

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

The header says which: `github.com/prisma/docs@v6 (branch v6 for major 6)`. After upgrading lockdocs, run `lockdocs fetch` again to refresh copies made by an older version. For explicit `fetch` / `--fetch` only, `GITHUB_TOKEN` (or `GH_TOKEN`) is optional and is sent to api.github.com only (the REST API). It is never sent to github.com (git), codeload.github.com or raw.githubusercontent.com, so private repositories cannot be read: a fetch for one fails with "private repository: not supported". Explicit docs-site fetches download the whole docs-site repository as a tarball and keep only the docs; over the cap they fall back to per-file requests. Docs-site date lookups (`before the next major`) still use the REST API, and a token raises its 60-requests-an-hour limit.

Release-tag enrichment already happens on first query. Separate website repositories above describe a major, not an exact patch version, and remain explicit-fetch-only; their provenance is included in the answer header.

## Packages that are not installed

With fetching enabled, a pinned package that is not on disk is downloaded at exactly that version (npm tarball, PyPI wheel or sdist, crates.io `.crate`, Go module zip), and you can ask about a version you do not use:

```bash
lockdocs npm:zod@4.1.5 "reject unknown keys" --fetch
```

Only docs and source files are unpacked (no binaries, nothing outside the target directory, 64 MB download cap). Git dependencies are never fetched from a registry.

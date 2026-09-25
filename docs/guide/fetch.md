# Fetching and upstream docs

lockdocs answers from what is on your machine. Two optional downloads make answers better; both happen once and are cached.

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

The header says which: `github.com/prisma/docs@v6 (branch v6 for major 6)`. After upgrading lockdocs, run `lockdocs fetch` again to refresh copies made by an older version. Set `GITHUB_TOKEN` to raise GitHub's API limit (60 requests an hour without it; a package takes 2 to 20).

To have it happen automatically on first query instead, enable fetching: `--fetch`, `LOCKDOCS_FETCH=1`, or register the MCP server with `lockdocs setup --fetch`.

## Packages that are not installed

With fetching enabled, a pinned package that is not on disk is downloaded at exactly that version (npm tarball, PyPI wheel or sdist, crates.io `.crate`, Go module zip), and you can ask about a version you do not use:

```bash
lockdocs npm:zod@4.1.5 "reject unknown keys" --fetch
```

Only docs and source files are unpacked (no binaries, nothing outside the target directory, 64 MB download cap). Git dependencies are never fetched from a registry.

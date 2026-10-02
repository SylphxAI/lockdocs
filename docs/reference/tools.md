# MCP tools

All tools take an optional `root` (absolute project path). `resolve` stays local.
`docs` and `api` may populate the cache with anonymous public release-tag docs
on first use; `offline: true` restricts that call to installed/cached files.
Start the server with `--offline` to prevent all downloads, including the model.

## resolve

List pinned versions and whether their docs are available.

| Argument | Type | |
|---|---|---|
| `filter` | string | Substring of a package name; also searches transitive dependencies |

Each row: ecosystem, `name@version`, direct or transitive, status (`docs ready`, `version drift`, `not installed`) and the source.

## docs

Answer a question from the installed version's docs and API reference.

| Argument | Type | |
|---|---|---|
| `query` | string, required | Words or identifiers |
| `package` | string | `zod`, `npm:zod`, `pydantic@2.9.2`, `tokio`, `github.com/gin-gonic/gin`; comma-separate several. Omit to search direct dependencies (a package named in the query is picked automatically) |
| `offline` | boolean | No package/upstream network access for this call |
| `tokens` | integer | Budget, default 1200 (200-20000) |
| `upgrade_to` | string | lockdocs Pro: the [upgrade report](/pro) from the pinned version of `package` to this version. Without a licence the result is a normal (not error) answer with `structuredContent.pro_required`. Reading any version's docs, with `package: "zod@4.0.0"`, stays free |

## api

Exact signature and doc comment of one symbol.

| Argument | Type | |
|---|---|---|
| `symbol` | string, required | `zod.z.object`, `z.object`, `tokio::spawn`, `axum::Router::route`, `pydantic.BaseModel.model_dump`, `gin.Context.JSON` |
| `package` | string | When the symbol has no package prefix |
| `offline` | boolean | No package/upstream network access for this call |
| `tokens` | integer | Budget, default 1200 |

Returns the best match (following re-exports), other declarations in the same file (overloads), members for classes, interfaces, structs and traits, and other matches. If no symbol has that name, it falls back to `docs`.

Add `"format": "json"` to any call for structured output.

Structured docs/api answers include `provenance`: the requested version, actual package source, registry-fetch flag, the upstream manifest (repository, tag, immutable commit, counts and notes), its display label, and any fetch failure note. Text answers include the same source and failure notes. Failed enrichment falls back only to the requested package version, never latest.

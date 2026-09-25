# MCP tools

All tools are read-only and take an optional `root` (absolute project path).

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
| `tokens` | integer | Budget, default 1200 (200-20000) |

## api

Exact signature and doc comment of one symbol.

| Argument | Type | |
|---|---|---|
| `symbol` | string, required | `zod.z.object`, `z.object`, `tokio::spawn`, `axum::Router::route`, `pydantic.BaseModel.model_dump`, `gin.Context.JSON` |
| `package` | string | When the symbol has no package prefix |
| `tokens` | integer | Budget, default 1200 |

Returns the best match (following re-exports), other declarations in the same file (overloads), members for classes, interfaces, structs and traits, and other matches. If no symbol has that name, it falls back to `docs`.

Add `"format": "json"` to any call for structured output.

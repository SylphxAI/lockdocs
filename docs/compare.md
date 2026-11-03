---
title: Lockdocs vs Context7 — installed versions and offline docs
description: Compare Lockdocs and Context7 by documentation source, version selection, offline use and query limits. Reproduce an exact-version example with release 0.7.0.
---

# Lockdocs vs Context7

Context7 retrieves documentation from a hosted index. Lockdocs reads your
lockfile and installed packages. Both are useful; the choice is mostly about
where the answer lives and how you select its version.

**Use Lockdocs** for the exact declarations shipped with your installed version,
for installed packages you do not publish, or when you need to query offline.
**Use Context7** for indexed website guides and examples, including libraries
you have not installed. You can run both in the same editor.

[Set up your coding client](./guide/setup) · [Benchmark method and results](./benchmarks)

## What differs

Context7 facts below refer to its own [README at the 26 October 2026
revision](https://github.com/upstash/context7/blob/5b623bfd383b3c0bb5479738caf74764e5280d76/README.md),
checked on **29 October 2026**. Service features and limits can change.

| | Lockdocs 0.7.0 | Context7 (documented service) |
|---|---|---|
| Documentation source | Installed READMEs, docs and declarations; optional release-tag upstream docs | Hosted documentation and code examples; the README describes the API backend, parser and crawler as separate from the public MCP client |
| Version selection | Exact version from the project's lockfile; installed-version drift is reported | The README documents mentioning a version in the prompt and selecting a library ID; it does not describe reading your local lockfile |
| API reference | Signatures and doc comments from `.d.ts`, Python, Rust and Go sources | Documentation snippets and examples retrieved with `query-docs` |
| Disconnected queries | Yes, from installed files or previously cached docs; `--offline` prohibits downloads | The documented MCP setup connects to `mcp.context7.com`; running its client locally is not an offline copy of that service |
| Query quota | No hosted query quota, account or API key for local `resolve`, `docs` and `api` queries; your machine's resources still apply | The README recommends a free API key for higher rate limits; check the current service terms rather than relying on a fixed call count here |

Lockdocs does not contain every guide. A package can ship few docs, and a symbol
match is not the same as a complete migration guide. Automatic upstream fetching
needs a connection the first time. For website-only material, Context7 may be
the better starting point. This page is a workflow comparison, not a new
head-to-head accuracy or latency measurement.

## Reproduce it: the installed version changes the answer

This example asks for the same `ZodObject.strict` declaration in two small npm
projects. It uses published **Lockdocs 0.7.0**, **zod 3.22.4** and **zod 4.1.5**.
Verified on Linux with Node.js 20.19.2 and npm 9.2.0 on **29 October 2026**. Initial npm
installation needs a connection. After that, the queries use the installed
binary directly, not `npx`, and work without fetching a model or upstream docs.

Run in a POSIX shell (macOS, Linux, or WSL):

```bash
# Create a new directory; nothing in an existing project is changed.
mkdir lockdocs-version-demo
cd lockdocs-version-demo
mkdir runner zod3 zod4
npm install --prefix runner --ignore-scripts --no-audit --no-fund @sylphx/lockdocs@0.7.0
BIN="$PWD/runner/node_modules/.bin/lockdocs"
"$BIN" version

(cd zod3 && npm init -y >/dev/null && npm install --ignore-scripts --no-audit --no-fund --save-exact zod@3.22.4)
(cd zod4 && npm init -y >/dev/null && npm install --ignore-scripts --no-audit --no-fund --save-exact zod@4.1.5)

# You can disconnect now. Use a fresh cache to avoid relying on previous queries.
export LOCKDOCS_CACHE="$PWD/empty-cache"
export LOCKDOCS_NO_SYSTEM_PYTHON=1
for project in zod3 zod4; do
  "$BIN" resolve -C "$PWD/$project" --offline
  "$BIN" api ZodObject.strict --pkg zod --tokens 300 -C "$PWD/$project" --offline
done
```

The relevant output from that run (paths and line numbers are from the pinned
packages):

````text
zod@3.22.4 · npm · node_modules/zod · pinned in package-lock.json

### ZodObject.strict (method) — zod@3.22.4 lib/types.d.ts:498
```ts
strict(message?: errorUtil.ErrMessage): ZodObject<T, "strict", Catchall>
```

zod@4.1.5 · npm · node_modules/zod · pinned in package-lock.json

### ZodObject.strict (method) — zod@4.1.5 v4/classic/schemas.d.ts:427
```ts
strict(): ZodObject<Shape, core.$strict>
```
Consider `z.strictObject(A.shape)` instead
````

The v4 result also lists the bundled v3 compatibility declaration under
"Other matches". Check the primary citation's version and path, not just the
symbol name. You can inspect both cited `.d.ts` files directly in `node_modules`.
`resolve` should report one direct dependency in each project, its exact version,
`package-lock.json`, and `docs ready`.

This demonstrates Lockdocs' installed-version and offline path. It is not a
claim that Context7 cannot answer the same question.

## Ask Context7 the same question

Connect Context7 using its [documented MCP setup](https://github.com/upstash/context7/blob/5b623bfd383b3c0bb5479738caf74764e5280d76/README.md#installation).
In a separate conversation, copy this prompt, then repeat with **4.1.5**:

```text
Use Context7 for zod 3.22.4. Resolve the library ID, then retrieve the exact
TypeScript signature of ZodObject.strict. Include the source and version.
If that exact release is not indexed, say which version the result covers.
```

For a direct MCP test, the corresponding first call is:

```json
{
  "name": "resolve-library-id",
  "arguments": {
    "libraryName": "zod",
    "query": "In zod 3.22.4, what is the exact TypeScript signature of ZodObject.strict?"
  }
}
```

Then call `query-docs` with the **`libraryId` returned by that resolution** and
the same question as `query`. Do not invent an exact-version ID if the service
does not return one. Both tool names and fields come from the dated README's
[Available Tools](https://github.com/upstash/context7/blob/5b623bfd383b3c0bb5479738caf74764e5280d76/README.md#available-tools)
section. Repeat both calls for 4.1.5.

Record the date, returned library ID, reported version and citations. Compare
the signature with the two installed files above. Treat a missing release or
an answer without a version citation as a limitation of that run, not proof
that another version's declaration is correct. Hosted responses can change as
the index changes; this page does not publish a new Context7 result or score.

## What “offline” and “no query quota” mean

- Install Lockdocs and the dependencies before going offline. npm can need a
  connection even when Lockdocs itself is configured with `--offline`.
- Installed files work without an embedding model: retrieval falls back to
  keyword search. Cached upstream docs also remain usable offline.
- If you need docs that the package does not ship, run `lockdocs fetch` while
  connected first. First-use upstream requests have their own provider limits;
  “no query quota” describes local queries, not unlimited downloads.
- No account is required for these local queries. This is separate from the
  optional Pro features; no purchase is needed to reproduce this page.

[Copy-paste MCP setup for Claude Code, Codex, Cursor, VS Code, Claude Desktop,
Windsurf and Gemini CLI →](./guide/setup)

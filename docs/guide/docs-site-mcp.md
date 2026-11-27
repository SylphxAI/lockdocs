---
title: Docs-site MCP servers and Lockdocs — who picks the version
description: Mintlify and ReadMe host an MCP server for a published docs site; Docusaurus needs a community plugin and a function host. Lockdocs answers from your lockfile. Run a Next.js 14 vs 16 cookies() example.
---

# Docs-site MCP servers and Lockdocs

A docs-site MCP server is run by whoever **publishes** the documentation. It
answers from what that site currently serves. Lockdocs runs in the project
that **consumes** the library. It answers from the version pinned in your
lockfile and installed on disk. They answer different questions, so you will
often want both.

**Use the publisher's MCP server** for product guides, hosted API references,
private docs behind the publisher's sign-in, and (on ReadMe) live calls to the
publisher's API. **Use Lockdocs** when the answer depends on the dependency
version your project installed. Lockdocs answers for npm, PyPI, crates.io and
Go dependencies with the same three tools.

[Set up Lockdocs in your coding client](./setup) · [Lockdocs vs Context7](../compare)

## Which docs platforms ship an MCP server

All facts below were checked on **27 November 2026** against each vendor's own
documentation, and by calling `initialize` and `tools/list` on a public
endpoint. Plans, limits and tool names can change.

| | Mintlify | ReadMe | Docusaurus |
|---|---|---|---|
| MCP server | Built in. Mintlify [generates a search MCP server](https://www.mintlify.com/docs/ai/model-context-protocol) for every site, at `/mcp` | Built in. Turn it on under **AI → MCP**; it is [hosted at `https://your-project.readme.io/mcp`](https://docs.readme.com/main/docs/mcp) | **None in Docusaurus itself.** The Docusaurus monorepo ([`meta/docusaurus` at commit `3430978`](https://github.com/meta/docusaurus/tree/343097822317fa1454e1d24e8c3429ae4b22a88c/packages), 26 Nov 2026) has no MCP package, and `https://docusaurus.io/mcp` returns 404 |
| How you get one | Nothing to deploy | One toggle in the dashboard | Install the community plugin [`docusaurus-plugin-mcp-server`](https://github.com/scalvert/docusaurus-plugin-mcp-server) (2.7.2, MIT), then deploy its handler to a function runtime |
| Static hosting | Not applicable (Mintlify hosts it) | Not applicable (ReadMe hosts it) | The plugin's docs say [GitHub Pages, S3 and other static hosts cannot answer MCP requests](https://docusaurus-plugin-mcp-server.vercel.app/docs/deploy/static-hosts). You run the endpoint on Vercel, Netlify, Cloudflare Workers, Deno, Bun or Node |
| Documented tools | Search, query docs filesystem, submit feedback | OpenAPI tools (`list-endpoints`, `get-endpoint`, `search-endpoints`, `execute-request`, `list-specs`, `get-server-variables`), plus `search` and `fetch` for guides, plus any custom tools you define | `docs_search`, `docs_fetch` |
| Plan notes | WebMCP needs Pro or Enterprise. Documented hourly rate limits apply per IP and per site | OpenAPI tools are on all plans. Docs `search` and `fetch` need the AI Booster Pack add-on | Free plugin (MIT); you run the function host yourself |
| Private docs | Yes, through `/authed/mcp` (OAuth, or client credentials for CI) | Yes, through the documented `x-readme-auth` header | Depends on how you deploy and protect the endpoint |
| Version selection | A `version` filter, shown only when the site has several versions. The AI tool chooses it from the conversation | `?branch=` picks a version or branch. Docs search tools are not available on branches | The live tools take `query` and `limit` (search) and `url` (fetch); there is no version argument |

Docusaurus maintainers [do not review, test or endorse](https://docusaurus.io/community/resources#community-plugins)
third-party plugins. If you choose one, treat it as a dependency you own.

### What the public endpoints answered

We sent `initialize` and `tools/list` to each endpoint over Streamable HTTP,
without credentials:

| Endpoint | Server name | Tools listed |
|---|---|---|
| `https://www.mintlify.com/docs/mcp` | `Mintlify` | `search_mintlify` (`query`, `language`), `query_docs_filesystem_mintlify`, `submit_feedback` |
| `https://docs.readme.com/mcp` | `ReadMe Documentation` | Five of the six OpenAPI tools above (no `get-server-variables`), `search`, `fetch`, and three tools outside the documented list: `send-feedback`, `update-docs`, `draft-changelog` |
| `https://docusaurus-plugin-mcp-server.vercel.app/mcp` (the plugin's own docs) | `docusaurus-plugin-mcp-server` 2.7.2 | `docs_search` (`query`, `limit`), `docs_fetch` (`url`) |
| `https://docusaurus.io/mcp` | — | HTTP 404 |

Mintlify's own site has one version, so its search tool offers `language` and
no `version` filter, as its documentation describes. Tool names are
site-specific: Mintlify asks clients to read them from the server rather than
hardcode them.

## The boundary: whose version is it?

A docs-site MCP server runs on the publisher's infrastructure. It does not see
your project, so it does not read your lockfile. It serves the version that the
publisher deployed. If the publisher keeps several versions, the agent must
pick one from the prompt.

Lockdocs runs on your machine, in your project:

- It reads `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`,
  `Cargo.lock`, `uv.lock`, `poetry.lock` and the other formats in
  [Ecosystems](./ecosystems), and finds each package's installed files.
- It reports version drift between the lockfile and the installed files
  rather than hiding it.
- `api` returns the exact declaration from the installed package, cited as
  `package@version path:line`.
- `docs` searches the installed package's README, changelog and docs. Unless
  you turn [fetching](./fetch) off, the first query also adds public docs from
  your version's release tag. Answers state where each section came from.

What Lockdocs does not do: it does not replace publisher-hosted guides that are
not in the package or at the matching release tag, it does not call the
vendor's API, and it does not see docs that sit behind the vendor's sign-in.
For those, connect the publisher's MCP server.

## Try it: Next.js 14 and 16 answer `cookies()` differently

In Next.js 14, `cookies()` from `next/headers` returns the cookie store
directly. Since `v15.0.0-RC` it is an async function that returns a `Promise`,
according to the version history in the Next.js docs. An agent that remembers
the wrong one writes code that does not match your installed types. This example
installs **next@14.2.35** and **next@16.4.6** in two new projects and asks
Lockdocs for the same declaration in each.

Run in a POSIX shell (macOS, Linux or WSL) with Node 20.9 or later (the
minimum next@16.4.6 declares; the example never starts Next.js). The npm
installs need a connection. The Lockdocs queries below run with `--offline`
and a new, empty cache.

```bash
# Creates a new directory; nothing in an existing project changes.
mkdir lockdocs-next-demo
cd lockdocs-next-demo
mkdir runner next14 next16
npm install --prefix runner --ignore-scripts --no-audit --no-fund @sylphx/lockdocs@0.7.12
BIN="$PWD/runner/node_modules/.bin/lockdocs"
"$BIN" version

(cd next14 && npm init -y >/dev/null && npm install --ignore-scripts --no-audit --no-fund --save-exact next@14.2.35)
(cd next16 && npm init -y >/dev/null && npm install --ignore-scripts --no-audit --no-fund --save-exact next@16.4.6)

# You can disconnect now.
export LOCKDOCS_CACHE="$PWD/empty-cache"
export LOCKDOCS_NO_SYSTEM_PYTHON=1
for project in next14 next16; do
  "$BIN" api cookies --pkg next/headers --tokens 300 -C "$PWD/$project" --offline
done
```

Output recorded with Lockdocs 0.7.12 on 27 November 2026:

````text
next@14.2.35 · npm · node_modules/next · pinned in package-lock.json
Public import: `next/headers` · symbol `cookies`

Declaration entrypoint: headers.d.ts (legacy)

### cookies (function) — next@14.2.35 dist/client/components/headers.d.ts:13
```ts
export declare function cookies(): ReadonlyRequestCookies
```
next@16.4.6 · npm · node_modules/next · pinned in package-lock.json
Public import: `next/headers` · symbol `cookies`

Declaration entrypoint: headers.d.ts (legacy)

### cookies (function) — next@16.4.6 dist/server/request/cookies.d.ts:2
```ts
export declare function cookies(): Promise<ReadonlyRequestCookies>
```
````

Use `--pkg next/headers`, the path you import from. With `--pkg next`, Lockdocs
reports `not_exported`, because the root `next` entrypoint does not export
`cookies`, and lists `next/headers` among the alternate imports. `(legacy)` here
describes how the declaration file was found. Next.js maps `next/headers` with a
top-level `headers.d.ts` rather than an `exports` entry. It does not mean the
API is deprecated. You can open both cited files in `node_modules` to check.

Your agent gets the same answer over MCP. Ask it to call `api` with
`symbol: "cookies"` and `package: "next/headers"`. The server's tools are
`resolve`, `docs` and `api`.

### Add the guides for each version

Declarations show the signature. For the surrounding guide, run a `docs` query
while connected. The first query adds the public docs at the release tag:

```bash
export LOCKDOCS_CACHE="$PWD/online-cache"
export LOCKDOCS_EMBED=0   # keyword search only; skips the 129 MB model download
for project in next14 next16; do
  "$BIN" docs "cookies await async request API" --pkg next --tokens 600 -C "$PWD/$project"
done
```

On 27 November 2026 the two headers read:

```text
next@14.2.35 · npm · node_modules/next · pinned in package-lock.json
Upstream docs: github.com/vercel/next.js@v14.2.35 commit 7b940d9ce96faddb9f92ff40f5e35c34ace04eb2 (317 files)

next@16.4.6 · npm · node_modules/next · pinned in package-lock.json
Upstream docs: github.com/vercel/next.js@v16.4.6 commit 54b876b6b161b0ecda19562eb264bec637e60d6d (476 files)
```

The 14.2.35 answer leads with a Pages Router section, then the App Router
Route Handlers guide at that tag, where the example is
`const cookieStore = cookies()`. The 16.4.6 answer leads with
`next@16.4.6 dist/docs/01-app/03-api-reference/04-functions/cookies.md:1`,
which describes `cookies` as an **async** function and uses
`await cookies()`. That file is shipped inside the installed package.

### What Next.js already gives you

Next.js publishes its docs on nextjs.org, not on Mintlify, ReadMe or
Docusaurus, and `https://nextjs.org/mcp` returns 404. Next.js covers part of
this itself. According to the guides bundled with next@16.4.6:

- Next.js **16.2 and later** ship version-matched docs in
  `node_modules/next/dist/docs/`. From 16.3, when `next dev` detects a coding
  agent, it writes an `AGENTS.md` block that points the agent at them.
- The [`next-devtools-mcp`](https://github.com/vercel/next-devtools-mcp)
  package connects agents to the dev server's `/_next/mcp` endpoint (errors,
  routes, logs). Its `nextjs_docs` tool points the agent at the bundled docs.
- `nextjs.org/docs` serves every page as Markdown, plus `llms.txt` and
  `llms-full.txt`.

So for a project on Next.js 16.2 or later, you do not need Lockdocs to find
version-matched Next.js guides. Lockdocs adds exact declarations, a token
budget and line citations. It also gives one answer path for every other
dependency in the lockfile, and release-tag docs for Next.js 16.1 and earlier.
(For those versions, Next.js also documents a legacy `agents-md` command that
downloads matching docs into `.next-docs/`.) The two work together.

## Inspect a docs-site MCP server yourself

This lists the tools on Mintlify's public endpoint. Replace the URL with any
site's `/mcp` endpoint. A private site needs the credentials its vendor
documents.

```bash
curl -sS -X POST https://www.mintlify.com/docs/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  --data '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

The reply arrives as a server-sent event: an `event: message` line, then
`data:` followed by the JSON result. Look at each search tool's
`inputSchema`. If it has no version argument, the server answers for whatever
the site currently publishes. If it has one, check which values are valid and
make sure your agent passes the one your project uses.

## Choosing

- **You publish docs** and want agents to read them: Mintlify and ReadMe give
  you a hosted server with no deployment. On Docusaurus, add a community MCP
  plugin and a function host, or publish `llms.txt` and Markdown pages (also
  through community plugins) and let agents fetch them.
- **You consume a library** and the version matters: Lockdocs. It needs no
  per-library setup and answers from installed files offline.
- **You consume a product with an API and hosted guides** (often a ReadMe or
  Mintlify site): connect the publisher's server for guides and API calls, and
  keep Lockdocs for the SDK version you installed.
- **Both apply**: connect both. The AI decides which server to call for each
  question. Mintlify's guide notes that each search adds results to the
  conversation, so connect only the servers relevant to your work.

This page compares where answers come from and how the version is chosen. It
does not measure the accuracy of any docs-site MCP server, and it does not
claim one cannot answer a version-specific question when the publisher
maintains that version.

[Copy-paste Lockdocs setup for Claude Code, Codex, Cursor, VS Code, Claude
Desktop, Windsurf and Gemini CLI →](./setup)

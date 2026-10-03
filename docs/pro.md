---
title: lockdocs Pro
description: The upgrade report and private sources for teams that pin versions. Everything free in lockdocs stays free, under MIT.
---

# lockdocs Pro

Your agent already gets the right docs for every package you install, free,
forever. Pro gives your team a clear answer to "what breaks if we upgrade?",
and the same exact-version docs for the packages your company keeps private.

## Upgrade report

Ask for `next 15` from `next 14` and get every API you actually call that was
removed, renamed, re-signed or deprecated, with the call sites in your code,
both versions cited `package@version path:line`, and the migration guide
sections that apply.

```sh
lockdocs upgrade next 15.0.0
```

Agents ask for the same report through the `docs` tool's `upgrade_to`
argument. Reading another version's docs and its migration guide stays free.

## Private sources

Your internal packages are not on npmjs.org, PyPI, crates.io or the Go proxy,
and a hosted doc service cannot read them. lockdocs Pro reads them from where
you already keep them, using the credentials you already have. There is no new
config file and no account: it looks at the files and variables your package
manager and git already use.

| Source | Where lockdocs looks |
| --- | --- |
| npm | `.npmrc` (project, user, `NPM_CONFIG_*`): `@scope:registry=`, `//host/:_authToken`, `_auth`; the `resolved` URL in `package-lock.json` |
| PyPI | uv (`uv.toml`, `[[tool.uv.index]]`, `[tool.uv.sources]`, `UV_INDEX_*`), pip (`PIP_INDEX_URL`, `PIP_EXTRA_INDEX_URL`, `pip.conf`), credentials in the URL or `~/.netrc`; the registry in `uv.lock` |
| Cargo | alternate registries in `.cargo/config.toml` (sparse protocol), `~/.cargo/credentials.toml` or `CARGO_REGISTRIES_<NAME>_TOKEN`; the registry in `Cargo.lock` |
| Go | `GOPROXY`, `GOPRIVATE`, `GONOPROXY`, `GONOSUMDB` (environment or `go env`), `~/.netrc`; private modules without a proxy are read from their git host at the version's tag |
| Git hosts | docs at the version's tag from GitHub, GitHub Enterprise, GitLab and Bitbucket (and any https git host on the same pattern), with `GH_TOKEN`/`GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN`, `GITLAB_TOKEN`, `BITBUCKET_TOKEN`, or whatever `git credential fill` returns for that host |

Where a lockfile records the registry a package came from, lockdocs uses that
one. It works inside the tools you already use: `resolve`, `docs` and `api`
for agents, and `lockdocs fetch` on the command line. Private downloads happen
when fetching is on (`--fetch` or `LOCKDOCS_FETCH=1`, or `lockdocs fetch`),
like any registry download.

A credential goes only to the exact host (scheme, host and port) it is
configured for. lockdocs follows redirects one hop at a time and looks the
credential up again for each hop, so a redirect to another host, a plain-http
downgrade or a tarball served from a CDN receives nothing. Credentials are
never printed, logged or stored in the cache; errors name the host that
refused, never what was sent. Cargo's git-protocol registries are not read
(use the sparse protocol), and a download URL on a different host than the
registry's index is fetched without the registry's token.

Without Pro, a request that needs a private source gets the same answer as
the upgrade report: a normal result saying private sources are part of
lockdocs Pro, with the link. Nothing is sent to the private host.

## Price

**{{ $site.themeConfig.pro.price }} per seat per year**, paid upfront. No
per-call charges, no parsing fees, no usage caps. Seats come in packs of 1, 5,
10, 25 and 50. CI runners don't count as seats. Pro includes email support with
a reply within two business days.

Context7 Pro is US$10 a seat a month, plus US$5 per 1,000 calls over 2,000 and
US$5 per million tokens to parse private repositories (context7.com/plans,
checked 2026-10-02). lockdocs Pro is one yearly price, and it answers for the
version you pinned.

<p v-if="$site.themeConfig.pro.checkoutUrl"><a :href="$site.themeConfig.pro.checkoutUrl">Buy lockdocs Pro</a></p>

`lockdocs licence buy` opens the Pro page; in-terminal purchase turns on when checkout is live.

Already bought? Run `lockdocs licence activate <token>` (it verifies the token
offline and stores it for you), or set `LOCKDOCS_LICENCE_TOKEN` in CI. Check it
with `lockdocs licence status`, which shows where the token was read and warns
"expires in N days" during the last 30 days; the upgrade report adds a renewal
line then too. Without a valid licence `lockdocs upgrade` exits 3 (so does
`lockdocs fetch` or a named-package query that needs a private source), and an agent
calling `upgrade_to` gets a normal answer saying the report is part of lockdocs
Pro, with the link.

Everything free in lockdocs stays free, under MIT.

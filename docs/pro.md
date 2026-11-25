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

Not on Pro yet? The same command still answers, locally and free: it tells you
how many of the APIs your project calls change between the two versions (removed,
renamed, re-signed, deprecated), across how many call sites, and shows one as a
sample. Pro adds the full list: each change, its replacement, your call sites and
the migration sections.

```text
next 14.2.3 -> 15.0.0: 7 of the APIs your project calls change (3 removed, 2 renamed, 2 deprecated) across 12 call sites. Sample: removed next.foo at app/page.tsx:8.
```

The preview downloads nothing the report would not: the target version comes
from your installed or cached files, or from a fetch you turned on.

Agents ask for the same report through the `docs` tool's `upgrade_to`
argument. Reading another version's docs and its migration guide stays free.

## Private sources

Your internal packages are not on npmjs.org, PyPI, crates.io or the Go proxy,
and a hosted doc service cannot read them. lockdocs Pro reads them from where
you already keep them, using the files, variables and credentials your package
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

Lockdocs Pro costs **US$120/yr for one seat**, tax included and billed annually
upfront. Checkout is available in USD, GBP, EUR and HKD.

[See Lockdocs Pro pricing](/pricing) to buy online. After checkout, run
`lockdocs login` and sign in with the account you used to buy Pro. The CLI
verifies and stores your licence.

If you already have a licence token, run `lockdocs licence activate <token>`
to verify and store it offline, or set `LOCKDOCS_LICENCE_TOKEN` in CI. Check
your licence with `lockdocs licence status`. It shows where the token was read and
warns when the licence expires within 30 days. The upgrade report also shows a
renewal link during that period.

Without a valid licence, `lockdocs upgrade` exits 3. So does `lockdocs fetch`
or a named-package query that needs a private source. An agent calling
`upgrade_to` gets the preview above and the Pro link, marked `pro_required`.

Everything free in lockdocs stays free, under MIT.

## Licence verification and test mode

Every licence is verified offline, on your machine, and keeps the plan, seats
and expiry it was bought with. The CLI and agents over MCP apply the same
signature, plan, product and expiry checks.

`lockdocs licence --test status` and `lockdocs licence --test activate <token>`
work with a test licence and report `Pro (TEST)`. Test mode reads
`LOCKDOCS_TEST_LICENCE_TOKEN` and its own `test-licence` file, leaving your
`LOCKDOCS_LICENCE_TOKEN` and `licence` file untouched. It does not buy a licence
or unlock Pro. Keep real tokens out of shell history and logs.

---
title: lockdocs Pro
description: The upgrade report for teams that pin versions. Everything free in lockdocs stays free, under MIT.
---

# lockdocs Pro

Your agent already gets the right docs for every package you install, free,
forever. Pro gives your team a clear answer to "what breaks if we upgrade?".

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

Already bought? Run `lockdocs licence activate <token>`, or set
`LOCKDOCS_LICENCE_TOKEN` in CI.

Everything free in lockdocs stays free, under MIT.

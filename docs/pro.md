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

Not on Pro yet? The same command still answers, locally and free: it tells you
how many of the APIs your project calls change between the two versions (removed,
renamed, re-signed, deprecated), across how many call sites, and shows one as a
sample. Pro adds the full list: each change, its replacement, your call sites and
the migration sections.

```text
next 14.2.3 -> 15.0.0: 7 of the APIs your project calls change (3 removed, 2 renamed, 2 deprecated) across 12 call sites. Sample: removed next.foo at app/page.tsx:8. The full report ... is lockdocs Pro (US$120/seat/yr): <this page>
```

The preview downloads nothing the report would not: the target version comes
from your installed or cached files, or from a fetch you turned on.

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

`lockdocs licence buy` starts the device flow for lockdocs Pro: open the link
printed in your terminal to buy, then return to the terminal. After checkout,
lockdocs verifies and stores your licence automatically.

Already bought? Run `lockdocs licence activate <token>` (it verifies the token
offline and stores it for you), or set `LOCKDOCS_LICENCE_TOKEN` in CI. Check it
with `lockdocs licence status`, which shows where the token was read and warns
"expires in N days" during the last 30 days; the upgrade report adds a renewal
line then too. Without a valid licence `lockdocs upgrade` exits 3, and an agent
calling `upgrade_to` gets a normal answer with the preview above and the Pro
link, marked `pro_required` so the agent can tell you.

Everything free in lockdocs stays free, under MIT.

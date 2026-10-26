---
title: Recover your lockdocs Pro licence
description: Retrieve an existing lockdocs Pro licence through Sylphx Money.
money_checkout: pro
---

# Recover your lockdocs Pro licence

Enter the email address used at checkout. If a paid licence exists, Money sends
an expiring recovery link. The response never says whether that address has a
purchase, and no licence token is shown to an unverified browser.

The recovery link returns only to this product's site. Open it within one hour
to view and copy your licence token, then activate it on your machine:

```sh
lockdocs licence activate '<your token>'
lockdocs licence status
```

For CI, set `LOCKDOCS_LICENCE_TOKEN` instead. Existing licences keep their
original product, plan, seats and expiry. Do not share a licence token or a
recovery link in a public issue, log or repository.

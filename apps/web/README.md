# Lockdocs web preview

Run from the repo root:

```sh
cd apps/web
npm ci
npm run dev
```

The container build context is the repository root. The documentation is generated from the existing technical Markdown at build time; the legacy storefront is intentionally not imported. The catalogue is read from the same source as the existing Money workflow. The app doesn't provision catalogue items, mutate Access, configure a domain, or alter GitHub Pages.

## Preview operator setup

1. Verify that the deployment targets the **preview** environment and its platform hostname. Use the exact `LOCKDOCS_APP_ORIGIN` HTTPS origin for the frontend. Do not add public-domain routing or activate push-to-production.
2. Enable Auth on the preview environment. The web service expects the injected `SYLPHX_AUTH_URL`, `SYLPHX_AUTH_ORGANIZATION_ID` (Auth instance), `SYLPHX_PUBLISHABLE_KEY`, and `SYLPHX_AUTH_SECRET_KEY`. For custom provider credentials, set `LOCKDOCS_TENANT_AUTH_ORIGIN` to the exact verified HTTPS Account Portal origin for this environment. Enable GitHub then Google; allow the web origin and `/api/auth/callback` as appropriate for the provider policy. Enable Passkey for that exact RP ID and enroll a passkey before testing the passkey flow. Credentials enrolled for a different RP ID will not work here.
3. Configure `SYLPHX_MONEY_URL` and `SYLPHX_MONEY_PARENT` (the preview `orgs/…/projects/…/envs/…` path), verify that the preview catalogue is synced, and confirm that its Stripe connection is in **test mode**. Do not dispatch the repository’s Money catalogue workflow in write mode for preview setup: it also targets production. If the preview catalogue needs changes, use the authorized preview-only operator path. Bind an environment-scoped checkout-write `SYLPHX_MONEY_API_KEY` directly to the web service via Platform Secret Binding. No repository secret mapping. Money's end-user grant, licence, and portal reads use the Auth session exchange and that environment's publishable key, not the service secret.
4. Before checkout enablement, have the commercial owner approve and set the exact receipt wording in `LOCKDOCS_CCR_TERMS_TEXT` and a revision in `LOCKDOCS_CCR_TERMS_VERSION`. Blank/unconfigured checkout fails closed. The UI never treats a return URL as payment proof; it reads the account-bound entitlement from Money.
5. Point the branch CLI at the HTTPS preview with `LOCKDOCS_WEB_URL=https://<preview>.sylphx.app lockdocs login`. The CLI verifies the returned Money signature against its existing product policy before it writes anything. The browser callback contains a short-lived encrypted envelope; the native CLI must supply the verifier to open it. The envelope is stateless and may be redeemed again only with the same verifier during its 90-second lifetime. **Do not** wire a production signing key into preview to make staging grants pass. A grant signed by an untrusted preview signer cannot unlock a stock binary. Verify against the right approved preview trust policy before claiming real staging CLI activation.

Local unauthenticated use does not need provider credentials. The offline browser workbench never uploads pasted lockfiles. Never write real secrets into repository files or test fixtures.

## Optional site analytics

Set the runtime `LOCKDOCS_GA4_MEASUREMENT_ID` to a GA4 web measurement ID to enable this integration. Production declares its public ID in `sylphx.toml`; previews and local builds are off unless configured. Country defaults use Cloudflare's same-origin `/cdn-cgi/trace` and fail closed when it isn't available. Set both `LOCKDOCS_GOOGLE_ADS_ID` (an `AW-*` destination) and `LOCKDOCS_GOOGLE_ADS_PURCHASE_LABEL` (the purchase action label) to enable Ads conversions after explicit Ads consent. If either is missing or invalid, Ads stays off and GA4 is unaffected. The purchase check uses the existing server-only Money key (needs checkout create plus `billing:read`) and a verified Auth session. A signed browser cookie suppresses repeats; Google Ads receives the same transaction ID across devices. Tests use synthetic IDs and intercept all Google traffic. Never use a live checkout to test analytics.

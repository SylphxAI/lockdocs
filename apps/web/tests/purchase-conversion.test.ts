import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { createHash } from "node:crypto";
import type { APIEvent } from "@solidjs/start/server";
import { GET as analyticsConfig } from "../src/routes/api/analytics/config";
import { GET as frame } from "../src/routes/api/analytics/tag";
import { POST } from "../src/routes/api/billing/purchase-conversion";
import { purchaseConversionPath } from "../src/lib/google-ads";
import { signalChoiceCookie } from "../src/lib/signal-privacy";

const app = "https://lockdocs.example.test";
const identity = "https://auth.example.test";
const money = "https://money.example.test";
const parent = "orgs/fixture/projects/fixture/envs/live";
const checkoutName = parent + "/checkout_sessions/chk-fixture";
const sid = "cs_live_fixture";
const session = "__Host-lockdocs-session=identity_org_session_fixture";
const auth = "fixture-money-credential";
const adId = "AW-1234567890";
const label = "fixture_A-B_1";
const config = {
  LOCKDOCS_APP_ORIGIN: app, LOCKDOCS_GA4_MEASUREMENT_ID: "G-TEST123456",
  LOCKDOCS_GOOGLE_ADS_ID: adId, LOCKDOCS_GOOGLE_ADS_PURCHASE_LABEL: label,
  SYLPHX_AUTH_URL: identity, SYLPHX_AUTH_ORGANIZATION_ID: "fixture-instance",
  SYLPHX_AUTH_SECRET_KEY: "fixture-auth", SYLPHX_PUBLISHABLE_KEY: "fixture-publishable-key",
  SYLPHX_MONEY_URL: money, SYLPHX_MONEY_PARENT: parent, SYLPHX_MONEY_API_KEY: auth,
};

function env(t: TestContext) {
  for (const [key, value] of Object.entries(config)) {
    const prior = process.env[key];
    process.env[key] = value;
    t.after(() => { if (prior === undefined) delete process.env[key]; else process.env[key] = prior; });
  }
}

const event = (request: Request) => ({ request }) as APIEvent;

test("exposes a strictly valid paired Ads config, independently of GA4", async t => {
  env(t);
  const req = event(new Request(app + "/api/analytics/config"));
  const read = async () => await analyticsConfig(req).json();
  assert.deepEqual(await read(), { measurementId: "G-TEST123456", googleAdsId: adId, googleAdsPurchaseLabel: label, region: null, gpc: false });
  for (const [id, purchaseLabel] of [
    [undefined, label], [adId, undefined], [adId, ""], ["", label], ["AW-12345", label], ["AW-1234567890123456", label],
    ["aw-123456", label], ["AW-123456x", label], [adId + "\n", label], [" " + adId, label], [adId, "path/label"],
    [adId, label + " "], [adId, "x".repeat(65)],
  ]) {
    if (id === undefined) delete process.env.LOCKDOCS_GOOGLE_ADS_ID; else process.env.LOCKDOCS_GOOGLE_ADS_ID = id;
    if (purchaseLabel === undefined) delete process.env.LOCKDOCS_GOOGLE_ADS_PURCHASE_LABEL;
    else process.env.LOCKDOCS_GOOGLE_ADS_PURCHASE_LABEL = purchaseLabel;
    assert.deepEqual(await read(), { measurementId: "G-TEST123456", region: null, gpc: false });
    assert.equal(frame(req).status, 200);
  }
  delete process.env.LOCKDOCS_GA4_MEASUREMENT_ID;
  assert.equal(frame(req).status, 404);
  process.env.LOCKDOCS_GOOGLE_ADS_ID = adId;
  process.env.LOCKDOCS_GOOGLE_ADS_PURCHASE_LABEL = label;
  assert.equal(frame(req).status, 200);
  assert.equal(frame(event(new Request(req.request.url, { headers: { "Sec-GPC": "1" } }))).status, 404);
});

function fixture(t: TestContext) {
  env(t);
  const checkout: Record<string, unknown> = {
    name: checkoutName, subject: { end_user: "usr_reader" }, processor_session_id: sid, livemode: true,
    state: "complete", payment_status: "paid", amount_total: 8250, currency_code: "GBP",
  };
  const redeem = { status: 200, body: { paid: true, checkout_session: checkoutName } };
  const calls: string[] = [];
  let user = "usr_reader";
  t.mock.method(globalThis, "fetch", async (url: URL, init: RequestInit) => {
    calls.push(url.href);
    if (url.href === identity + "/v1/client/session") {
      assert.equal(new Headers(init.headers).get("authorization"), "Bearer identity_org_session_fixture");
      return Response.json({ user: { id: user }, session: { id: "fixture-session", instance_id: config.SYLPHX_AUTH_ORGANIZATION_ID,
        expires_at: Math.floor(Date.now() / 1000) + 300 } });
    }
    assert.equal(new Headers(init.headers).get("authorization"), "Bearer " + auth);
    assert.equal(init.redirect, "error");
    if (url.href === money + "/v1/money/checkout_sessions:redeem") {
      assert.equal(init.method, "POST");
      assert.deepEqual(JSON.parse(String(init.body)), { checkout_session: sid });
      return Response.json(redeem.body, { status: redeem.status });
    }
    assert.equal(url.href, money + "/v1/" + checkoutName);
    assert.equal(init.method ?? "GET", "GET");
    return Response.json(checkout);
  });
  function claim({ origin = app as string | null, cookie = session, choice = "granted", gpc = "0", body = { session_id: sid, ads_consent: true } as object } = {}) {
    const headers = new Headers({ "Content-Type": "application/json", "Sec-GPC": gpc, "CF-IPCountry": "US",
      cookie: cookie + "; " + signalChoiceCookie + "=" + choice });
    if (origin !== null) headers.set("Origin", origin);
    return POST(event(new Request(app + purchaseConversionPath, { method: "POST", headers, body: JSON.stringify(body) })));
  }
  return { claim, calls, redeem, checkout, asUser: (id: string) => { user = id; } };
}

const expectedTransactionId = createHash("sha256").update("lockdocs-ads-purchase:" + checkoutName).digest("hex").slice(0, 32);

test("returns a verified conversion once per signed, user-bound claim cookie", async t => {
  const { claim, checkout, asUser } = fixture(t);
  const first = await claim();
  assert.equal(first.status, 200);
  assert.deepEqual(await first.json(), { transactionId: expectedTransactionId, value: 82.5, currency: "GBP" });
  assert.equal(first.headers.get("cache-control"), "private, no-store");
  const claimed = first.headers.get("set-cookie")!;
  assert.match(claimed, /^__Secure-lockdocs-ads-claimed=[a-f0-9]{32}\.[a-f0-9]{64}; /);
  assert.ok(claimed.endsWith("; Path=" + purchaseConversionPath + "; Secure; HttpOnly; SameSite=Lax"));
  const pair = claimed.split(";", 1)[0];
  const reload = await claim({ cookie: session + "; " + pair });
  assert.equal(reload.status, 204);
  assert.equal(reload.headers.get("set-cookie"), null);
  const tampered = await claim({ cookie: session + "; " + pair + "a" });
  assert.equal(tampered.status, 200);
  asUser("usr_other");
  checkout.subject = { end_user: "usr_other" };
  assert.equal((await claim({ cookie: session + "; " + pair })).status, 200);
});

test("uses Stripe presentment units for each paid currency", async t => {
  const { claim, checkout } = fixture(t);
  for (const [currency, value] of [["USD", 82.5], ["GBP", 82.5], ["EUR", 82.5], ["HKD", 82.5], ["JPY", 8250], ["KWD", 8.25], ["ISK", 82.5]] as const) {
    checkout.currency_code = currency;
    const result = await claim();
    assert.deepEqual(await result.json(), { transactionId: expectedTransactionId, value, currency });
  }
});

test("denies non-origin, unsigned-in, or nonconsenting requests before calling Money", async t => {
  const { claim, calls } = fixture(t);
  for (const [options, status] of [
    [{ origin: null }, 403], [{ origin: app + "/" }, 403], [{ origin: "https://cross.example.test" }, 403],
    [{ cookie: "" }, 401], [{ choice: "denied" }, 403], [{ choice: "analytics_only" }, 403], [{ choice: "" }, 403],
    [{ gpc: "1" }, 403], [{ body: { session_id: sid, ads_consent: false } }, 403], [{ body: { session_id: sid, ads_consent: "true" } }, 403],
    [{ body: { session_id: "cs_test_fixture", ads_consent: true } }, 204],
    [{ body: { session_id: "cs_live_invalid/path", ads_consent: true } }, 204],
  ] as const) {
    const response = await claim(options);
    assert.equal(response.status, status);
    assert.equal(response.headers.get("set-cookie"), null);
    assert.equal(response.headers.get("cache-control"), "private, no-store");
  }
  delete process.env.LOCKDOCS_GOOGLE_ADS_PURCHASE_LABEL;
  assert.equal((await claim()).status, 204);
  assert.ok(calls.every(url => url === identity + "/v1/client/session"));
});

test("refuses any unverified settlement, mismatched resource or invalid amount", async t => {
  const { claim, checkout, redeem, calls } = fixture(t);
  for (const [key, value] of [
    ["name", checkoutName + "-wrong"], ["subject", { end_user: "usr_other" }], ["processor_session_id", "cs_live_other"],
    ["livemode", false], ["state", "open"], ["payment_status", "unpaid"], ["denied_reason", "blocked"], ["amount_total", 0],
    ["amount_total", -42], ["amount_total", 1.1], ["amount_total", "8250"], ["currency_code", "gbp"],
  ] as const) {
    const initial = checkout[key];
    const response = await (checkout[key] = value, claim());
    assert.equal(response.status, 204, key);
    assert.equal(response.headers.get("set-cookie"), null);
    checkout[key] = initial;
  }
  for (const result of [
    { status: 201, body: redeem.body }, { status: 200, body: { ...redeem.body, paid: false } },
    { status: 200, body: { paid: true, checkout_session: checkoutName + "/../elsewhere" } },
    { status: 200, body: { paid: true, checkout_session: checkoutName.replace("/envs/live/", "/envs/other/") } },
  ]) {
    Object.assign(redeem, result);
    calls.length = 0;
    assert.equal((await claim()).status, 204);
    assert.deepEqual(calls, [identity + "/v1/client/session", money + "/v1/money/checkout_sessions:redeem"]);
  }
});

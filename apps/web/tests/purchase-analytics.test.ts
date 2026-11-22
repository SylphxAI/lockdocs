import assert from "node:assert/strict";
import { test } from "node:test";
import { checkout, verifiedPurchase } from "../src/lib/money";

const origin = "https://lockdocs.example.test";
const identity = "https://identity.example.test";
const money = "https://money.example.test";
const parent = "orgs/fixture/projects/fixture/envs/test";
const product = "lockdocs_pro_seat_year";
const id = "checkout_fixture";
const collection = parent + "/checkout_sessions";
const resource = collection + "/" + id;
const owner = "usr_fixture";
const config = {
  LOCKDOCS_APP_ORIGIN: origin, LOCKDOCS_GA4_MEASUREMENT_ID: "G-TEST123456",
  SYLPHX_AUTH_URL: identity, SYLPHX_AUTH_ORGANIZATION_ID: "fixture-instance",
  SYLPHX_AUTH_SECRET_KEY: "fixture-auth", SYLPHX_PUBLISHABLE_KEY: "fixture-public",
  SYLPHX_MONEY_URL: money, SYLPHX_MONEY_PARENT: parent, SYLPHX_MONEY_API_KEY: "fixture-money",
  LOCKDOCS_CCR_TERMS_TEXT: "Fixture terms only.", LOCKDOCS_CCR_TERMS_VERSION: "fixture-version",
};
const session = "__Host-lockdocs-session=identity_org_session_fixture";
const paid = {
  name: resource, subject: { end_user: owner }, line_items: [{ price: product, quantity: 1 }],
  state: "complete", payment_status: "paid", currency_code: "GBP", amount_total: "9900", amount_tax: "1650",
  processor_session_id: "synthetic_processor_session", buyer_email: "buyer@example.test",
};

function request(path: string, cookies: string, form?: Record<string, string>, extraHeaders?: Record<string, string>) {
  return new Request(origin + path, { method: "POST", headers: { origin, cookie: cookies, ...extraHeaders },
    body: form && new URLSearchParams(form) });
}

test("purchase requires a signed, account-bound checkout and confirmation of that exact payment", async t => {
  const previous = Object.fromEntries(Object.keys(config).map(key => [key, process.env[key]]));
  Object.assign(process.env, config);
  t.after(() => { for (const [key, value] of Object.entries(previous)) {
    if (value === undefined) delete process.env[key]; else process.env[key] = value;
  } });

  let subject = owner;
  let createName = resource;
  let result: object = paid;
  let reads = 0;
  t.mock.method(globalThis, "fetch", async (url: URL, init: RequestInit) => {
    if (url.href === identity + "/v1/client/session") return Response.json({
      session: { id: "fixture-session", instance_id: config.SYLPHX_AUTH_ORGANIZATION_ID, expires_at: Math.floor(Date.now() / 1000) + 300 },
      user: { id: subject },
    });
    if (url.href === money + "/v1/events/attribution_bindings") return new Response(null, { status: 204 });
    assert.equal(new Headers(init.headers).get("authorization"), "Bearer " + config.SYLPHX_MONEY_API_KEY);
    if (url.href === money + "/v1/" + collection) {
      assert.equal(init.method, "POST");
      assert.equal(JSON.parse(String(init.body)).subject.end_user, owner);
      return Response.json({ name: createName, url: "https://checkout.stripe.com/fixture" });
    }
    assert.equal(url.href, money + "/v1/" + resource);
    assert.equal(init.method ?? "GET", "GET");
    reads++;
    return Response.json(result);
  });
  const start = (measure: boolean, extraHeaders?: Record<string, string>) => checkout(request("/api/billing/checkout", session,
    { currency: "GBP", ...(measure ? { checkout_consent: "analytics" } : {}) }, extraHeaders));
  const verify = (cookies: string, extraHeaders?: Record<string, string>) => verifiedPurchase(request("/api/analytics/purchase", cookies, undefined, extraHeaders));

  assert.match((await start(false)).headers.get("set-cookie")!, /Max-Age=0/);
  assert.match((await start(true, { "sec-gpc": "1" })).headers.get("set-cookie")!, /Max-Age=0/);
  createName = collection + "/not/a/checkout";
  assert.match((await start(true)).headers.get("set-cookie")!, /Max-Age=0/);
  createName = resource;
  const redirect = await start(true);
  assert.equal(redirect.status, 303);
  assert.equal(redirect.headers.get("location"), "https://checkout.stripe.com/fixture");
  const proof = redirect.headers.get("set-cookie")!;
  assert.match(proof, /^__Host-lockdocs-analytics-checkout=.+; Max-Age=86400; Path=\/; Secure; HttpOnly; SameSite=Lax$/);
  const cookie = proof.split(";", 1)[0];
  const signedIn = session + "; " + cookie;

  assert.equal((await verify(cookie)).status, 401);
  assert.equal((await verify(session)).status, 404);
  assert.equal((await verify(signedIn, { origin: "https://unrelated.example.test" })).status, 403);
  assert.equal((await verify(signedIn, { "sec-gpc": "1" })).status, 403);
  assert.equal((await verify(signedIn.replace(id, "other_checkout"))).status, 404);
  subject = "usr_other";
  assert.equal((await verify(signedIn)).status, 404);
  subject = owner;
  assert.equal(reads, 0);

  for (const patch of [{ state: "open", payment_status: "unpaid" }, { payment_status: "unpaid" }]) {
    result = { ...paid, ...patch };
    const pending = await verify(signedIn);
    assert.equal(pending.status, 202);
    assert.equal(pending.headers.has("set-cookie"), false);
    assert.deepEqual(await pending.json(), {});
  }
  for (const patch of [
    { name: collection + "/different" }, { subject: { end_user: "usr_other" } },
    { line_items: [{ price: "other_sku", quantity: 1 }] }, { denied_reason: "country_denied" },
    { payment_status: "revoked" }, { payment_status: "no_payment_required" }, { amount_total: 0 }, { currency_code: "JPY" },
  ]) {
    result = { ...paid, ...patch };
    assert.equal((await verify(signedIn)).status, 404);
  }
  result = paid;
  const success = await verify(signedIn);
  assert.equal(success.status, 200);
  assert.equal(success.headers.get("cache-control"), "no-store");
  assert.match(success.headers.get("set-cookie")!, /Max-Age=0/);
  const payload = await success.json();
  assert.match(payload.transaction_id, /^[0-9a-f]{40}$/);
  assert.deepEqual(payload, { transaction_id: payload.transaction_id, currency: "GBP", value: 82.5 });
  assert.deepEqual(await (await verify(signedIn)).json(), payload, "concurrent returns use the same GA deduplication ID");
  const next = (await start(true)).headers.get("set-cookie")!.split(";", 1)[0];
  assert.notEqual((await (await verify(session + "; " + next)).json()).transaction_id, payload.transaction_id);
});

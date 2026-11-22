import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import { test } from "node:test";
import type { APIEvent } from "@solidjs/start/server";
import { checkout, portal } from "../src/lib/money";
import { POST as consent } from "../src/routes/api/cli/consent";
import { POST as exchange } from "../src/routes/api/cli/exchange";

const origin = "https://preview.example.test";
const auth = "https://identity.example.test";
const money = "https://billing.example.test";
const parent = "orgs/example/projects/example/envs/preview";
const subject = "usr_fixture";
const product = "lockdocs_pro_seat_year";
const config = {
  LOCKDOCS_APP_ORIGIN: origin, SYLPHX_AUTH_URL: auth, SYLPHX_AUTH_ORGANIZATION_ID: "preview-instance",
  SYLPHX_AUTH_SECRET_KEY: "fixture-auth", SYLPHX_PUBLISHABLE_KEY: "fixture-public",
  SYLPHX_MONEY_URL: money, SYLPHX_MONEY_PARENT: parent, SYLPHX_MONEY_API_KEY: "fixture-checkout",
  LOCKDOCS_CCR_TERMS_TEXT: "Fixture terms only.", LOCKDOCS_CCR_TERMS_VERSION: "fixture-revision",
};
const expiry = 2_000_000_000;
const cookie = "__Host-lockdocs-session=identity_org_session_fixture";
const fixtureGrant = "synthetic-licence-value";

type Expected = { url: string; method?: string; bearer?: string; publicKey?: boolean; body?: object; result: object; status?: number };
const inspect: Expected = {
  url: auth + "/v1/client/session", bearer: "identity_org_session_fixture",
  result: { session: { id: "fixture-session", instance_id: config.SYLPHX_AUTH_ORGANIZATION_ID, expires_at: expiry }, user: { id: subject } },
};
const token: Expected = {
  url: auth + "/v1/client/token", method: "POST", bearer: "identity_org_session_fixture", body: {},
  result: { access_token: "fixture.access", expires_in: 300 },
};
const scoped = (operation: string) => money + "/v1/money/" + parent + "/" + operation;
const entitlement: Expected = {
  url: scoped("entitlement_grants:getMine?feature=" + product), bearer: "fixture.access", publicKey: true,
  result: { entitled: true, subject_id: subject },
};
function post(path: string, form: Record<string, string>, headers = { origin, cookie }) {
  return new Request(origin + path, { method: "POST", headers, body: new URLSearchParams(form) });
}
const event = (request: Request) => ({ request }) as APIEvent;

test("billing and CLI consent use verified account scope and separate provider credentials", async (t) => {
  const previous = Object.fromEntries(Object.keys(config).map((key) => [key, process.env[key]]));
  Object.assign(process.env, config);
  t.after(() => { for (const [key, value] of Object.entries(previous)) {
    if (value === undefined) delete process.env[key]; else process.env[key] = value;
  } });
  const expected: Expected[] = [];
  const seen: Array<{ url: string; method: string; authorization: string | null; publicKey: string | null; body: unknown }> = [];
  t.mock.method(globalThis, "fetch", (url: URL, init: RequestInit) => {
    const headers = new Headers(init.headers);
    seen.push({ url: url.href, method: init.method ?? "GET", authorization: headers.get("authorization"), publicKey: headers.get("x-sylphx-key"), body: init.body ? JSON.parse(String(init.body)) : undefined });
    const stub = expected[seen.length - 1];
    return Promise.resolve(Response.json(stub?.result ?? {}, { status: stub ? stub.status ?? 200 : 500 }));
  });
  assert.equal((await checkout(post("/api/billing/checkout", { currency: "GBP" }, { origin: "https://unrelated.example.test", cookie }))).status, 403);
  assert.equal(seen.length, 0);
  expected.push(inspect, {
    url: money + "/v1/" + parent + "/checkout_sessions", method: "POST", bearer: "fixture-checkout",
    body: { subject: { end_user: subject }, line_items: [{ price: product, quantity: 1 }], currency_code: "GBP",
      metadata: { entry_surface: "pricing" },
      success_url: origin + "/dashboard?billing=return", cancel_url: origin + "/pricing",
      consent_collection: { terms_required: true, terms_text: config.LOCKDOCS_CCR_TERMS_TEXT, terms_version: config.LOCKDOCS_CCR_TERMS_VERSION } },
    result: { url: "https://checkout.stripe.com/example" },
  });
  assert.equal((await checkout(post("/api/billing/checkout", { currency: "GBP" }))).headers.get("location"), "https://checkout.stripe.com/example");
  expected.push(inspect, token, {
    url: scoped("portal_sessions:userCreate"), method: "POST", bearer: "fixture.access", publicKey: true,
    body: { subject: { end_user: subject }, return_url: origin + "/dashboard" },
    status: 412, result: { code: "INVALID_STATE", details: [{ reason: "no_customer" }] },
  });
  assert.equal((await portal(post("/api/billing/portal", {}))).headers.get("location"), origin + "/dashboard?billing=no_customer");

  const verifier = randomBytes(32);
  const state = randomBytes(32).toString("base64url");
  const query = { state, port: "44191", challenge: createHash("sha256").update(verifier).digest("base64url") };
  const authorize = () => consent(event(post("/api/cli/consent", query)));
  expected.push(inspect, token, { ...entitlement, result: { entitled: false, subject_id: subject } });
  assert.equal((await authorize()).headers.get("location"), origin + "/dashboard?cli=no_pro");
  const licenceUrl = new URL(scoped("licence_tokens:listMine"));
  licenceUrl.searchParams.set("filter", 'end_user = "usr_fixture" AND token_product = "lockdocs"');
  expected.push(inspect, token, entitlement, token, {
    url: licenceUrl.href, bearer: "fixture.access", publicKey: true,
    result: { next_page_token: "", licence_tokens: [{ token: fixtureGrant, token_product: "lockdocs", issue_time: "2020-01-01T00:00:00Z", expire_time: new Date(expiry * 1000).toISOString(), seats: 1 }] },
  });
  const callback = new URL((await authorize()).headers.get("location")!);
  assert.equal(callback.origin + callback.pathname, "http://127.0.0.1:44191/callback");
  assert.equal(callback.searchParams.get("state"), state);
  assert.ok(!callback.href.includes(fixtureGrant));
  const code = callback.searchParams.get("code");
  assert.ok(code);
  const redeem = (proof: Buffer) => exchange(event(new Request(origin + "/api/cli/exchange", {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ code, state, verifier: proof.toString("base64url") }),
  })));
  assert.equal((await redeem(randomBytes(32))).status, 400);
  const response = await redeem(verifier);
  assert.equal(response.headers.get("cache-control"), "no-store");
  assert.deepEqual(await response.json(), { grant: fixtureGrant });
  assert.deepEqual(seen, expected.map(({ url, method, bearer, publicKey, body }) => ({
    url, method: method ?? "GET", authorization: bearer ? "Bearer " + bearer : null, publicKey: publicKey ? config.SYLPHX_PUBLISHABLE_KEY : null, body,
  })));
});

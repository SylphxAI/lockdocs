import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { checkout } from "../src/lib/money";
import { attemptIntent, checkoutIntent } from "../src/lib/checkout-intent";

const app = "https://lockdocs.example.test";
const identity = "https://identity.example.test";
const api = "https://sylphx.example.test";
const parent = "orgs/fixture/projects/fixture/envs/test";
const stripe = "https://checkout.stripe.com/fixture";
const config = {
  LOCKDOCS_APP_ORIGIN: app, LOCKDOCS_GA4_MEASUREMENT_ID: "G-7FKD0Q6R5H",
  SYLPHX_AUTH_URL: identity, SYLPHX_AUTH_ORGANIZATION_ID: "fixture-instance",
  SYLPHX_AUTH_SECRET_KEY: "fixture-auth", SYLPHX_PUBLISHABLE_KEY: "fixture-publishable-key",
  SYLPHX_MONEY_URL: api, SYLPHX_MONEY_PARENT: parent, SYLPHX_MONEY_API_KEY: "fixture-money-key",
  LOCKDOCS_CCR_TERMS_TEXT: "Fixture terms only.", LOCKDOCS_CCR_TERMS_VERSION: "fixture-version",
};
const intentA = "a".repeat(64);
const intentB = "0123456789abcdef".repeat(4);

function fixture(t: TestContext) {
  const previous = Object.fromEntries(Object.keys(config).map(name => [name, process.env[name]]));
  Object.assign(process.env, config);
  t.after(() => { for (const [name, value] of Object.entries(previous)) {
    if (value === undefined) delete process.env[name]; else process.env[name] = value;
  } });
  const creates: { key: string | null; body: unknown }[] = [];
  const bindings: unknown[] = [];
  const money = { user: "usr_checkout_fixture", reply: (): Response => Response.json({ name: parent + "/checkout_sessions/chk_fixture", url: stripe }) };
  t.mock.method(globalThis, "fetch", async (url: URL, init: RequestInit) => {
    if (url.href === identity + "/v1/client/session") return Response.json({
      session: { id: "fixture-session", instance_id: config.SYLPHX_AUTH_ORGANIZATION_ID, expires_at: Math.floor(Date.now() / 1000) + 300 },
      user: { id: money.user },
    });
    if (url.href === api + "/v1/" + parent + "/checkout_sessions") {
      creates.push({ key: new Headers(init.headers).get("idempotency-key"), body: JSON.parse(String(init.body)) });
      return money.reply();
    }
    bindings.push(JSON.parse(String(init.body)));
    return Response.json({ binding_id: "binding_fixture", checkout_session_id: "chk_fixture", processor_session_id: null,
      expires_at: "2026-11-24T00:00:00Z" }, { status: 201 });
  });
  async function start(intents: (string | File)[] = [], currency = "GBP") {
    const body = new FormData();
    body.set("currency", currency);
    body.set("entry_surface", "pricing");
    for (const intent of intents) body.append("checkout_intent", intent);
    const headers = { origin: app, "CF-IPCountry": "US", cookie: "__Host-lockdocs-session=identity_org_session_fixture; _ga=GA1.1.123456789.1788573000" };
    const response = await checkout(new Request(app + "/api/billing/checkout", { method: "POST", headers, body }));
    await new Promise(resolve => setImmediate(resolve));
    return { status: response.status, location: response.headers.get("location") };
  }
  return { creates, bindings, money, start };
}

test("the same intent sends the same scoped key; another intent, buyer or currency sends another", async t => {
  const { creates, money, start } = fixture(t);
  const redirect = { status: 303, location: stripe };
  assert.deepEqual(await start([intentA]), redirect);
  assert.deepEqual(await start([intentA]), redirect);
  assert.deepEqual(await start([intentB]), redirect);
  assert.deepEqual(await start([intentA], "USD"), redirect);
  money.user = "usr_other_buyer";
  assert.deepEqual(await start([intentA]), redirect);
  const keys = creates.map(({ key }) => key);
  assert.equal(keys[0], keys[1]);
  assert.equal(new Set(keys).size, 4);
  for (const key of keys) {
    assert.match(key ?? "", /^lockdocs-lockdocs_pro_seat_year-[a-f0-9]{64}$/);
    for (const raw of [intentA, intentB, "usr_"]) assert.ok(!key?.includes(raw));
  }
});

test("a missing, duplicate or malformed intent checks out exactly as before, without a key", async t => {
  const { creates, bindings, start } = fixture(t);
  const baseline = await start();
  assert.deepEqual(baseline, { status: 303, location: stripe });
  for (const intents of [[""], [intentA.toUpperCase()], [intentA.slice(1)], [intentA + "0"], [" " + intentA], ["g".repeat(64)],
    [intentA, intentA], [intentA, intentB], [new File([intentA], "intent")]]) {
    assert.deepEqual(await start(intents), baseline);
  }
  assert.equal(creates.length, 10);
  for (const { key, body } of creates) {
    assert.equal(key, null);
    assert.deepEqual(body, creates[0].body);
  }
  assert.equal(bindings.length, 10);
  for (const binding of bindings) assert.deepEqual(binding, bindings[0]);
});

test("an intent changes neither the Money body, the attribution binding nor any return path", async t => {
  const { creates, bindings, money, start } = fixture(t);
  assert.deepEqual(await start([intentA]), await start());
  assert.deepEqual(creates[0].body, creates[1].body);
  assert.deepEqual(creates[0].body && (creates[0].body as { success_url: string }).success_url,
    app + "/dashboard?billing=return&session_id={CHECKOUT_SESSION_ID}");
  assert.deepEqual(bindings[0], bindings[1]);
  const unavailable = { status: 303, location: app + "/pricing?currency=GBP&billing=unavailable" };
  for (const reply of [() => new Response(null, { status: 500 }), () => new Response(null, { status: 409 }),
    () => Response.json({ url: "https://example.test/not-stripe" }), () => { throw new TypeError("network"); }]) {
    money.reply = reply;
    assert.deepEqual(await start([intentA]), unavailable);
    assert.deepEqual(await start(), unavailable);
  }
});

function memoryStorage(entries = new Map<string, string>()) {
  return () => ({ getItem: (key: string) => entries.get(key) ?? null, setItem: (key: string, value: string) => { entries.set(key, value); } }) as Storage;
}

test("the browser continues an attempt only on a failed return for the same currency and surface", () => {
  const storage = memoryStorage();
  const first = attemptIntent("GBP:pricing", false, storage);
  assert.equal(checkoutIntent(first), first);
  assert.equal(attemptIntent("GBP:pricing", true, storage), first);
  assert.equal(attemptIntent("GBP:pricing", true, storage), first);
  const usd = attemptIntent("USD:pricing", true, storage);
  assert.notEqual(usd, first);
  const fresh = attemptIntent("USD:pricing", false, storage);
  assert.notEqual(fresh, usd);
  assert.equal(attemptIntent("USD:pricing", true, storage), fresh);
});

test("unavailable or stale browser storage still mints a valid intent", () => {
  const broken = () => { throw new DOMException("denied", "SecurityError"); };
  const stale = memoryStorage(new Map([["lockdocs.checkout.intent", "{not json"]]));
  const tampered = memoryStorage(new Map([["lockdocs.checkout.intent", JSON.stringify({ scope: "GBP:pricing", intent: "x" })]]));
  for (const storage of [broken, stale, tampered]) {
    const intent = attemptIntent("GBP:pricing", true, storage);
    assert.equal(checkoutIntent(intent), intent);
  }
});

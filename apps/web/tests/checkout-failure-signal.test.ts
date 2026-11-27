import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test, type TestContext } from "node:test";
import { checkout } from "../src/lib/money";
import { signalChoiceCookie } from "../src/lib/signal-privacy";
import { continuesAttempt } from "../src/lib/checkout-intent";

const app = "https://lockdocs.example.test";
const identity = "https://identity.example.test";
const api = "https://sylphx.example.test";
const signal = "https://signal.example.test";
const parent = "orgs/fixture/projects/fixture/envs/test";
const key = "sylphx_pk_failure_fixture";
const config = {
  LOCKDOCS_APP_ORIGIN: app, SYLPHX_AUTH_URL: identity, SYLPHX_AUTH_ORGANIZATION_ID: "fixture-instance",
  SYLPHX_AUTH_SECRET_KEY: "fixture-auth", SYLPHX_PUBLISHABLE_KEY: key, SYLPHX_API_URL: signal,
  SYLPHX_MONEY_URL: api, SYLPHX_MONEY_PARENT: parent, SYLPHX_MONEY_API_KEY: "fixture-money-key",
  LOCKDOCS_CCR_TERMS_TEXT: "Fixture terms only.", LOCKDOCS_CCR_TERMS_VERSION: "fixture-version",
};
const intent = "c".repeat(64);
const granted = signalChoiceCookie + "=granted";
const eventId = (value: string) => createHash("sha256").update("lockdocs:checkout_failed:" + value).digest("hex");
const settle = async () => { for (let i = 0; i < 5; i++) await new Promise(resolve => setImmediate(resolve)); };

function fixture(t: TestContext) {
  const previous = Object.fromEntries(Object.keys(config).map(name => [name, process.env[name]]));
  Object.assign(process.env, config);
  t.after(() => { for (const [name, value] of Object.entries(previous)) {
    if (value === undefined) delete process.env[name]; else process.env[name] = value;
  } });
  const creates: unknown[] = [];
  const events: { url: string; headers: Record<string, string>; body: unknown }[] = [];
  const money = { reply: (): Response | Promise<Response> => new Response(null, { status: 422 }) };
  const ingest = { reply: (): Promise<Response> => Promise.resolve(Response.json({}, { status: 200 })) };
  t.mock.method(globalThis, "fetch", async (url: URL, init: RequestInit) => {
    if (url.href === identity + "/v1/client/session") return Response.json({
      session: { id: "fixture-session", instance_id: config.SYLPHX_AUTH_ORGANIZATION_ID, expires_at: Math.floor(Date.now() / 1000) + 300 },
      user: { id: "usr_failure_fixture" },
    });
    if (url.href === api + "/v1/" + parent + "/checkout_sessions") {
      creates.push(JSON.parse(String(init.body)));
      return money.reply();
    }
    events.push({ url: url.href, headers: Object.fromEntries(new Headers(init.headers)), body: JSON.parse(String(init.body)) });
    return ingest.reply();
  });
  async function start({ cookies = [granted], gpc, blocked, form = {} }: {
    cookies?: string[]; gpc?: string; blocked?: boolean; form?: Record<string, string | undefined>;
  } = {}) {
    const fields = { currency: "GBP", entry_surface: "pricing", checkout_intent: intent, ...form };
    const body = new URLSearchParams(Object.entries(fields).filter((entry): entry is [string, string] => entry[1] !== undefined));
    if (blocked) body.append("signal_blocked", "1");
    const headers = new Headers({ origin: app, "CF-IPCountry": "DE",
      cookie: ["__Host-lockdocs-session=identity_org_session_fixture", ...cookies].join("; ") });
    if (gpc !== undefined) headers.set("Sec-GPC", gpc);
    const response = await checkout(new Request(app + "/api/billing/checkout", { method: "POST", headers, body }));
    await settle();
    return { status: response.status, location: response.headers.get("location") };
  }
  return { creates, events, money, ingest, start };
}

const unavailable = (query = "currency=GBP") => ({ status: 303, location: app + "/pricing?" + query + "&billing=unavailable" });
// A definite refusal created no session, so the return tells the browser to start a new attempt.
const refused = (query = "currency=GBP") => ({ status: 303, location: app + "/pricing?" + query + "&billing=unavailable&attempt=done" });

test("Money's definite refusals send one consented checkout_failed with only status and surface", async t => {
  const { events, money, start } = fixture(t);
  const cases = [[400, "rejected"], [404, "rejected"], [412, "rejected"], [422, "rejected"],
    [401, "refused"], [403, "refused"], [429, "rate_limited"]] as const;
  for (const [status, outcome] of cases) {
    money.reply = () => Response.json({ code: "PRIVATE_DETAIL" }, { status });
    assert.deepEqual(await start(), refused());
    const [event, ...rest] = events.splice(0);
    assert.deepEqual(rest, []);
    assert.equal(event.url, signal + "/v1/analytics/events:batch");
    assert.deepEqual(event.headers, { "content-type": "application/json", "x-sylphx-key": key });
    assert.deepEqual(event.body, { consent: "granted", events: [{ event_id: eventId(intent), distinct_id: eventId(intent),
      event_name: "checkout_failed", properties: { status: outcome, entry_surface: "pricing" } }] });
  }
  money.reply = () => new Response(null, { status: 422 });
  for (const choice of ["analytics_only", "granted"]) {
    assert.deepEqual(await start({ cookies: [signalChoiceCookie + "=" + choice], form: { entry_surface: "guide_cursor_claude_code" } }),
      refused("currency=GBP&entry_surface=guide_cursor_claude_code"));
  }
  assert.deepEqual(events.map(({ body }) => (body as { events: { properties: unknown }[] }).events[0].properties),
    [{ status: "rejected", entry_surface: "guide_cursor_claude_code" }, { status: "rejected", entry_surface: "guide_cursor_claude_code" }]);
  const other = "d".repeat(64);
  await start({ form: { checkout_intent: other } });
  assert.equal((events.at(-1)?.body as { events: { event_id: string }[] }).events[0].event_id, eventId(other));
  for (const { body } of events) for (const raw of [intent, other, "usr_", "PRIVATE"]) assert.ok(!JSON.stringify(body).includes(raw));
});

test("unknown outcomes and anything before Money send nothing", async t => {
  const { creates, events, money, start } = fixture(t);
  for (const reply of [...[408, 409, 500, 502, 503, 504].map(status => () => new Response(null, { status })),
    () => new Response("not json", { status: 200 }), () => Response.json({ url: "https://example.test/x" }), () => Response.json({}, { status: 201 }),
    () => { throw new TypeError("network"); }, () => { throw new DOMException("timed out", "TimeoutError"); }]) {
    money.reply = reply;
    assert.deepEqual(await start(), unavailable());
  }
  assert.equal(creates.length, 11);
  money.reply = () => new Response(null, { status: 422 });
  assert.equal((await start({ form: { currency: "XYZ" } })).status, 400);
  assert.equal((await start({ form: { entry_surface: "unknown" } })).status, 400);
  assert.equal(creates.length, 11);
  assert.deepEqual(events, []);
});

test("without an explicit analytics grant, under GPC or without a valid intent, nothing is sent", async t => {
  const { creates, events, start } = fixture(t);
  // The new-attempt marker is not analytics: it follows the refusal whatever the consent.
  for (const options of [{ cookies: [] }, { cookies: [signalChoiceCookie + "=denied"] }, { cookies: [signalChoiceCookie + "=invalid"] },
    { cookies: [granted, granted] }, { gpc: "1" }, { gpc: "0, 1" }, { blocked: true }]) {
    assert.deepEqual(await start(options), refused());
  }
  // Without a valid intent there is no attempt to finish: the return is exactly today's.
  for (const checkout_intent of [undefined, intent.toUpperCase(), "x"]) {
    assert.deepEqual(await start({ form: { checkout_intent } }), unavailable());
  }
  assert.equal(creates.length, 10);
  assert.deepEqual(events, []);
  process.env.SYLPHX_API_URL = "http://signal.example.test"; // Signal unconfigured; Money still is.
  assert.deepEqual(await start(), refused());
  assert.equal(creates.length, 11);
  assert.deepEqual(events, []);
});

test("a definite refusal starts a new browser attempt; an unknown outcome keeps it", async t => {
  const { money, start } = fixture(t);
  money.reply = () => new Response(null, { status: 422 });
  const reset = new URL((await start()).location!);
  assert.equal(continuesAttempt(reset.searchParams.get("billing"), reset.searchParams.get("attempt")), false);
  money.reply = () => new Response(null, { status: 503 });
  const retry = new URL((await start()).location!);
  assert.equal(continuesAttempt(retry.searchParams.get("billing"), retry.searchParams.get("attempt")), true);
  assert.equal(continuesAttempt(undefined, undefined), false);
});

test("a Signal failure or stall never changes or delays the buyer's response", async t => {
  const { events, ingest, start } = fixture(t);
  const warnings = t.mock.method(console, "warn", () => {});
  for (const reply of [() => Promise.reject(new Error("private " + key)), () => Promise.resolve(new Response(null, { status: 500 })),
    () => new Promise<Response>(() => {})]) {
    ingest.reply = reply;
    assert.deepEqual(await start(), refused());
  }
  assert.equal(events.length, 3);
  assert.equal(warnings.mock.callCount(), 0);
});

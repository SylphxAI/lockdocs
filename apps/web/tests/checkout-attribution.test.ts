import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { checkout } from "../src/lib/money";
import { campaignCookie } from "../src/lib/checkout-campaign";

const app = "https://lockdocs.example.test";
const identity = "https://identity.example.test";
const api = "https://sylphx.example.test";
const parent = "orgs/fixture/projects/fixture/envs/test";
const id = "checkout_fixture";
const location = "https://checkout.stripe.com/fixture";
const key = "fixture-publishable-key";
const config = {
  LOCKDOCS_APP_ORIGIN: app, LOCKDOCS_GA4_MEASUREMENT_ID: "G-7FKD0Q6R5H",
  SYLPHX_AUTH_URL: identity, SYLPHX_AUTH_ORGANIZATION_ID: "fixture-instance",
  SYLPHX_AUTH_SECRET_KEY: "fixture-auth", SYLPHX_PUBLISHABLE_KEY: key,
  SYLPHX_MONEY_URL: api, SYLPHX_MONEY_PARENT: parent, SYLPHX_MONEY_API_KEY: "fixture-money-key",
  LOCKDOCS_CCR_TERMS_TEXT: "Fixture terms only.", LOCKDOCS_CCR_TERMS_VERSION: "fixture-version",
};
const ga = "_ga=GA1.1.123456789.1788573000";
const gs1 = "_ga_7FKD0Q6R5H=GS1.1.1788573100.2.1.1788573200.0.0.0";
const gs2 = "_ga_7FKD0Q6R5H=GS2.1.g1%24t1788573200%24s1788573100%24o2";
const campaign = (raw: string) => campaignCookie + "=" + encodeURIComponent(raw);
const ads = [campaign("gclid=Click.1-x&gbraid=Braid_2&wbraid=Braid_3"),
  "_fbc=fb.1.1788573200000.FB-click", "_fbp=fb.1.1788573200000.789123"];
const canonical = "https://lockdocs.sylphx.com/pricing";

function fixture(t: TestContext) {
  const previous = Object.fromEntries(Object.keys(config).map(name => [name, process.env[name]]));
  Object.assign(process.env, config);
  t.after(() => { for (const [name, value] of Object.entries(previous)) {
    if (value === undefined) delete process.env[name]; else process.env[name] = value;
  } });
  const calls: { url: string; init: RequestInit }[] = [];
  const billing: { name?: string; checkout_session_id?: string; url: string } = { name: parent + "/checkout_sessions/" + id, url: location };
  let events = (_: RequestInit): Promise<Response> => Promise.resolve(new Response(null, { status: 204 }));
  t.mock.method(globalThis, "fetch", async (url: URL, init: RequestInit) => {
    if (url.href === identity + "/v1/client/session") return Response.json({
      session: { id: "fixture-session", instance_id: config.SYLPHX_AUTH_ORGANIZATION_ID, expires_at: Math.floor(Date.now() / 1000) + 300 },
      user: { id: "usr_checkout_fixture" },
    });
    if (url.href === api + "/v1/" + parent + "/checkout_sessions") {
      assert.equal(JSON.parse(String(init.body)).success_url, app + "/dashboard?billing=return");
      return Response.json(billing);
    }
    calls.push({ url: url.href, init });
    return events(init);
  });
  async function start(choices: string[] = [], cookies: string[] = [], gpc?: string) {
    const headers = new Headers({ origin: app, referer: app + "/private?gclid=not-from-referer",
      cookie: ["__Host-lockdocs-session=identity_org_session_fixture", ...cookies].join("; ") });
    if (gpc !== undefined) headers.set("Sec-GPC", gpc);
    const body = new URLSearchParams({ currency: "GBP" });
    choices.forEach(choice => body.append("checkout_consent", choice));
    const response = await checkout(new Request(app + "/api/billing/checkout?gclid=not-from-url", { method: "POST", headers, body }));
    assert.equal(response.status, 303);
    // Checkout must not set the retired browser purchase proof cookie.
    assert.equal(response.headers.get("set-cookie"), null);
    return response;
  }
  return { calls, start, billing, setEvents: (handler: typeof events) => { events = handler; } };
}

test("missing, rejected, malformed and ambiguous consent, GPC and disabled GA never bind", async t => {
  const { calls, start } = fixture(t);
  const cookies = [ga, gs1, ...ads];
  for (const choice of [[], ["reject"], ["essential"], ["granted"], ["all", "reject"], ["analytics", "analytics"]]) {
    assert.equal((await start(choice, cookies)).headers.get("location"), location);
  }
  for (const gpc of ["1", "0, 1"]) assert.equal((await start(["all"], cookies, gpc)).headers.get("location"), location);
  delete process.env.LOCKDOCS_GA4_MEASUREMENT_ID;
  assert.equal((await start(["all"], cookies)).headers.get("location"), location);
  assert.equal(calls.length, 0);
});

test("binds the Money resource ID to GS1 or GS2 and only includes ad identifiers with both ad grants", async t => {
  const { calls, start } = fixture(t);
  assert.equal((await start(["analytics"], [ga, gs1, ...ads])).headers.get("location"), location);
  assert.equal((await start(["all"], [ga, gs2, ...ads])).headers.get("location"), location);
  assert.equal(calls.length, 2);
  const actual = calls.map(({ url, init }) => {
    assert.equal(url, api + "/v1/events/attribution_bindings");
    assert.equal(init.method, "POST");
    assert.equal(init.redirect, "error");
    assert.deepEqual(Object.fromEntries(new Headers(init.headers)), { "content-type": "application/json", "x-sylphx-key": key });
    return JSON.parse(String(init.body));
  });
  const base = { checkout_session_id: id, ga_client_id: "123456789.1788573000", ga_session_id: "1788573100", page_location: canonical };
  assert.deepEqual(actual, [
    { ...base, consent: { analytics_storage: "granted", ad_storage: "denied", ad_user_data: "denied", ad_personalization: "denied" } },
    { ...base, consent: { analytics_storage: "granted", ad_storage: "granted", ad_user_data: "granted", ad_personalization: "denied" },
      gclid: "Click.1-x", gbraid: "Braid_2", wbraid: "Braid_3", fbc: "fb.1.1788573200000.FB-click", fbp: "fb.1.1788573200000.789123" },
  ]);
});

test("omits invalid, duplicate or oversized identifiers and never uses URL or other GA cookies as fallback", async t => {
  const { calls, start, billing } = fixture(t);
  const samples = [
    ["_ga=GA1.1.123.456%0A", "_ga_7FKD0Q6R5H=GS2.1.s123%24s456", "_ga_OTHER=GS1.1.789",
      campaign("gclid=a%2Fb&gbraid=a&gbraid=b&wbraid=%C3%A9"), "_fbc=fb.1.1788573200000.%0A", "_fbp=fb.1.1788573200000.x"],
    [ga, ga, gs1, gs2, campaign("gclid=" + "X".repeat(201) + "&wbraid=has+space&gbraid=%25"), ...ads.slice(1), ...ads.slice(1)],
    ["_ga=%FF", "_ga_7FKD0Q6R5H=GS2.1.o2%24s0", campaign("q=not_a_click")],
  ];
  for (const cookies of samples) await start(["all"], cookies);
  assert.equal(calls.length, samples.length);
  for (const { init } of calls) assert.deepEqual(Object.keys(JSON.parse(String(init.body))).sort(),
    ["checkout_session_id", "consent", "page_location"]);
  billing.name = parent + "/checkout_sessions/not/a/resource";
  assert.equal((await start(["all"], [ga, gs1])).headers.get("location"), location);
  assert.equal(calls.length, samples.length);
  delete billing.name;
  billing.checkout_session_id = id;
  assert.equal((await start(["analytics"], [ga, gs1])).headers.get("location"), location);
  assert.equal(JSON.parse(String(calls.at(-1)?.init.body)).checkout_session_id, id);
  assert.equal(calls.length, samples.length + 1);
});

test("Events timeout is 750 ms, checkout does not await Events, and diagnostics stay content-free", async t => {
  const { start, setEvents, calls } = fixture(t);
  const warnings = t.mock.method(console, "warn", () => {});
  const timeout = t.mock.method(AbortSignal, "timeout");
  let settle!: () => void;
  let timedOut = false;
  let signal: AbortSignal | null | undefined;
  setEvents(init => new Promise((_, reject) => {
    signal = init.signal;
    signal?.addEventListener("abort", () => { timedOut = true; reject(new Error(key + id + api + ga)); settle(); }, { once: true });
  }));
  const aborted = new Promise<void>(resolve => { settle = resolve; });
  const keepAlive = setTimeout(() => settle(), 2000); // Node's timeout signal is unref'ed.
  t.after(() => clearTimeout(keepAlive));
  assert.equal((await start(["analytics"], [ga, gs2])).headers.get("location"), location);
  assert.equal(calls.length, 1);
  assert.equal(signal?.aborted, false);
  assert.deepEqual(timeout.mock.calls.at(-1)?.arguments, [750]);
  await aborted;
  assert.equal(timedOut, true);
  assert.equal((signal?.reason as DOMException).name, "TimeoutError");

  setEvents(() => Promise.reject(new Error("private transport details " + key + id + api + ga)));
  assert.equal((await start(["analytics"], [ga])).headers.get("location"), location);
  let discarded = false;
  setEvents(async () => new Response(new ReadableStream({ cancel() { discarded = true; } }),
    { status: 502, statusText: "private-upstream-text", headers: { "x-private": key } }));
  assert.equal((await start(["analytics"], [ga])).headers.get("location"), location);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(discarded, true);
  assert.deepEqual(warnings.mock.calls.map(call => call.arguments), [
    ["Checkout attribution binding unavailable"], ["Checkout attribution binding unavailable"], ["Checkout attribution binding rejected"],
  ]);
});

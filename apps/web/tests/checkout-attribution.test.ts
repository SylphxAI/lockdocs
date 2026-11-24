import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { checkout } from "../src/lib/money";
import { signalChoiceCookie } from "../src/lib/signal-privacy";

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
const choice = (value: string) => signalChoiceCookie + "=" + value;
const touch = { gclid: "Click.1-x", gbraid: "Braid_2", wbraid: "Braid_3", utm_source: "release notes", arbitrary: "private" };
const ads = ["_fbc=fb.1.1788573200000.FB-click", "_fbp=fb.1.1788573200000.789123"];
const allGranted = { analytics_storage: "granted", ad_storage: "granted", ad_user_data: "granted", ad_personalization: "granted" };
const allDenied = { analytics_storage: "denied", ad_storage: "denied", ad_user_data: "denied", ad_personalization: "denied" };
const canonical = "https://lockdocs.sylphx.com/pricing";

function fixture(t: TestContext) {
  const previous = Object.fromEntries(Object.keys(config).map(name => [name, process.env[name]]));
  Object.assign(process.env, config);
  t.after(() => { for (const [name, value] of Object.entries(previous)) {
    if (value === undefined) delete process.env[name]; else process.env[name] = value;
  } });
  const calls: { url: string; init: RequestInit }[] = [];
  const checkoutMetadata: unknown[] = [];
  const billing: { name?: string; checkout_session_id?: string; url: string } = { name: parent + "/checkout_sessions/" + id, url: location };
  let events = (_: RequestInit): Promise<Response> => Promise.resolve(Response.json({
    binding_id: "binding_fixture", checkout_session_id: id, processor_session_id: null, expires_at: "2026-11-24T00:00:00Z",
  }, { status: 201 }));
  t.mock.method(globalThis, "fetch", async (url: URL, init: RequestInit) => {
    if (url.href === identity + "/v1/client/session") return Response.json({
      session: { id: "fixture-session", instance_id: config.SYLPHX_AUTH_ORGANIZATION_ID, expires_at: Math.floor(Date.now() / 1000) + 300 },
      user: { id: "usr_checkout_fixture" },
    });
    if (url.href === api + "/v1/" + parent + "/checkout_sessions") {
      const body = JSON.parse(String(init.body));
      assert.equal(body.success_url, app + "/dashboard?billing=return");
      checkoutMetadata.push(body.metadata);
      return Response.json(billing);
    }
    calls.push({ url: url.href, init });
    return events(init);
  });
  async function start({ cookies = [], gpc, region, attribution, blocked, body = new URLSearchParams({ currency: "GBP", entry_surface: "pricing" }) }: {
    cookies?: string[]; gpc?: string; region?: string; attribution?: unknown; blocked?: boolean; body?: URLSearchParams | FormData;
  } = {}) {
    const headers = new Headers({ origin: app, referer: app + "/private?gclid=not-from-referer",
      cookie: ["__Host-lockdocs-session=identity_org_session_fixture", ...cookies].join("; ") });
    if (gpc !== undefined) headers.set("Sec-GPC", gpc);
    if (region !== undefined) headers.set("CF-IPCountry", region);
    if (attribution !== undefined) body.append("signal_touch", JSON.stringify(attribution));
    if (blocked) body.append("signal_blocked", "1");
    const response = await checkout(new Request(app + "/api/billing/checkout?gclid=not-from-url", { method: "POST", headers, body }));
    // Checkout must not set the retired browser purchase proof cookie.
    assert.equal(response.headers.get("set-cookie"), null);
    return response;
  }
  return { calls, checkoutMetadata, start, billing, setEvents: (handler: typeof events) => { events = handler; } };
}

test("Money receives only the allowlisted pricing surface, even with untrusted form values", async t => {
  const { calls, checkoutMetadata, start } = fixture(t);
  for (const form of [undefined, new URLSearchParams({ currency: "GBP" })]) {
    const response = await start({ cookies: [ga, gs1], gpc: "1", body: form });
    assert.equal(response.status, 303);
    assert.equal(response.headers.get("location"), location);
    assert.deepEqual(checkoutMetadata.splice(0), [{ entry_surface: "pricing" }]);
  }
  for (const surfaces of [["dashboard"], [""], [" Pricing "], ["pricing\n"], ["定價"],
    ["/pricing?gclid=private"], ["x".repeat(101)], ["pricing", "pricing"], [new File([], "pricing")]]) {
    const form = new FormData();
    form.set("currency", "GBP");
    for (const surface of surfaces) form.append("entry_surface", surface);
    const response = await start({ cookies: [ga, gs1], body: form });
    assert.equal(response.status, 400);
    assert.deepEqual(await response.json(), { error: "invalid_entry_surface" });
    assert.deepEqual(checkoutMetadata, []);
  }
  assert.deepEqual(calls.map(({ init }) => JSON.parse(String(init.body))), [
    { checkout_session_id: id, consent: allDenied }, { checkout_session_id: id, consent: allDenied },
  ]);
});

for (const region of ["US", "CA", "AU"]) {
  test(region + " opts out by default and binds complete attribution without a stored choice", async t => {
    const { calls, start } = fixture(t);
    const response = await start({ region, cookies: [ga, gs2, ...ads], attribution: touch });
    assert.equal(response.headers.get("location"), location);
    assert.equal(calls.length, 1);
    const [{ url, init }] = calls;
    assert.equal(url, api + "/v1/events/attribution_bindings");
    assert.equal(init.method, "POST");
    assert.equal(init.redirect, "error");
    assert.deepEqual(Object.fromEntries(new Headers(init.headers)), { "content-type": "application/json", "x-sylphx-key": key });
    assert.deepEqual(JSON.parse(String(init.body)), {
      checkout_session_id: id, consent: allGranted, ga_client_id: "123456789.1788573000", ga_session_id: "1788573100",
      gclid: "Click.1-x", gbraid: "Braid_2", wbraid: "Braid_3", fbc: "fb.1.1788573200000.FB-click", fbp: "fb.1.1788573200000.789123",
      page_location: canonical + "?utm_source=release+notes",
    });
  });
}

for (const [name, privacy] of [
  ...["AX", "GF", "GP", "MQ", "MF", "RE", "YT"].map(region => [region, { region }] as const),
  ["EU", { region: "EU" }], ["Norway", { region: "NO" }], ["UK", { region: "GB" }], ["Switzerland", { region: "CH" }],
  ["unknown", { region: "XX" }], ["missing", {}], ["conflicting countries", { region: "DE, US" }],
  ["explicit opt-out", { region: "US", cookies: [choice("denied")] }],
  ["invalid choice", { region: "US", cookies: [choice("invalid")] }],
  ["ambiguous choice", { region: "US", cookies: [choice("granted"), choice("denied")] }],
  ["GPC overriding stored grant", { region: "US", gpc: "0, 1", cookies: [choice("granted")] }],
  ["GPC overriding implicit grant", { region: "CA", gpc: "1" }],
  ["unready or navigator GPC", { region: "AU", blocked: true }],
] as const) {
  test(name + " binds only the checkout identity and all-denied flags", async t => {
    const { calls, start } = fixture(t);
    const body = new URLSearchParams({ currency: "GBP", checkout_consent: "all" });
    assert.equal((await start({ ...privacy, body, cookies: [...("cookies" in privacy ? privacy.cookies : []), ga, gs1, ...ads], attribution: touch })).headers.get("location"), location);
    assert.equal(calls.length, 1);
    assert.deepEqual(JSON.parse(String(calls[0].init.body)), { checkout_session_id: id, consent: allDenied });
  });
}

test("explicit grants work in strict regions; analytics-only never includes ad identifiers", async t => {
  const { calls, start } = fixture(t);
  const base = { region: "DE", attribution: touch };
  await start({ ...base, cookies: [choice("analytics_only"), ga, gs1, ...ads] });
  await start({ ...base, cookies: [choice("granted"), ga, gs2, ...ads] });
  assert.equal(calls.length, 2);
  const [analytics, full] = calls.map(({ init }) => JSON.parse(String(init.body)));
  assert.deepEqual(analytics, {
    checkout_session_id: id, ga_client_id: "123456789.1788573000", ga_session_id: "1788573100",
    page_location: canonical + "?utm_source=release+notes",
    consent: { ...allDenied, analytics_storage: "granted" },
  });
  assert.deepEqual(full.consent, allGranted);
  assert.equal(full.gclid, touch.gclid);
  assert.equal(full.fbc, "fb.1.1788573200000.FB-click");
});

test("omits invalid or duplicate identifiers and never uses URL, referrer or other GA cookies as fallback", async t => {
  const { calls, start, billing } = fixture(t);
  const samples = [
    ["_ga=GA1.1.123.456%0A", "_ga_7FKD0Q6R5H=GS2.1.s123%24s456", "_ga_OTHER=GS1.1.789",
      "_fbc=fb.1.1788573200000.%0A", "_fbp=fb.1.1788573200000.x"],
    [ga, ga, gs1, gs2, ...ads, ...ads],
    ["_ga=%FF", "_ga_7FKD0Q6R5H=GS2.1.o2%24s0"],
  ];
  for (const cookies of samples) await start({ region: "US", cookies, attribution: { gclid: "a/b", gbraid: "%", wbraid: "has space" } });
  assert.equal(calls.length, samples.length);
  for (const { init } of calls) assert.deepEqual(JSON.parse(String(init.body)),
    { checkout_session_id: id, consent: allGranted, page_location: canonical });
  billing.name = parent + "/checkout_sessions/not/a/resource";
  assert.equal((await start()).headers.get("location"), location);
  assert.equal(calls.length, samples.length);
  delete billing.name;
  billing.checkout_session_id = id;
  assert.equal((await start()).headers.get("location"), location);
  assert.equal(JSON.parse(String(calls.at(-1)?.init.body)).checkout_session_id, id);
  assert.equal(calls.length, samples.length + 1);
  delete process.env.LOCKDOCS_GA4_MEASUREMENT_ID;
  assert.equal((await start({ region: "US" })).headers.get("location"), location);
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
  assert.equal((await start({ region: "US", cookies: [ga, gs2] })).headers.get("location"), location);
  assert.equal(calls.length, 1);
  assert.equal(signal?.aborted, false);
  assert.deepEqual(timeout.mock.calls.at(-1)?.arguments, [750]);
  await aborted;
  assert.equal(timedOut, true);
  assert.equal((signal?.reason as DOMException).name, "TimeoutError");

  setEvents(() => Promise.reject(new Error("private transport details " + key + id + api + ga)));
  assert.equal((await start({ region: "US", cookies: [ga] })).headers.get("location"), location);
  setEvents(async () => new Response(null,
    { status: 502, statusText: "private-upstream-text", headers: { "x-private": key } }));
  assert.equal((await start({ region: "US", cookies: [ga] })).headers.get("location"), location);
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(warnings.mock.calls.map(call => call.arguments), [
    ["Checkout attribution binding unavailable"], ["Checkout attribution binding unavailable"], ["Checkout attribution binding unavailable"],
  ]);
});

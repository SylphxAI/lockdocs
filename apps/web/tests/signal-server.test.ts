import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { signalConfig } from "../src/lib/analytics-config";
import { signalCheckoutStarted } from "../src/lib/signal-server";

const key = "sylphx_pk_signal_fixture";
const origin = "https://signal.example.test";
function configure(t: TestContext) {
  const names = ["SYLPHX_API_URL", "SYLPHX_PUBLISHABLE_KEY"];
  const prior = names.map(name => process.env[name]);
  t.after(() => names.forEach((name, i) => {
    if (prior[i] === undefined) delete process.env[name]; else process.env[name] = prior[i];
  }));
  process.env.SYLPHX_PUBLISHABLE_KEY = key;
  process.env.SYLPHX_API_URL = origin;
}

test("Signal config publishes only a publishable key and an exact HTTPS origin", t => {
  configure(t);
  delete process.env.SYLPHX_API_URL;
  assert.deepEqual(signalConfig(), { signalKey: key, signalApiOrigin: "https://api.sylphx.com" });
  process.env.SYLPHX_API_URL = "  " + origin + "/  ";
  process.env.SYLPHX_PUBLISHABLE_KEY = " " + key + " ";
  assert.deepEqual(signalConfig(), { signalKey: key, signalApiOrigin: origin });
  for (const bad of ["http://localhost", "/v1", "https://user@signal.example.test", origin + "/v1", origin + "?", origin + "#", "not a URL"]) {
    process.env.SYLPHX_API_URL = bad;
    assert.equal(signalConfig(), null);
  }
  process.env.SYLPHX_API_URL = origin;
  process.env.SYLPHX_PUBLISHABLE_KEY = "not-a-publishable-key";
  assert.equal(signalConfig(), null);
});

test("checkout batch is consent-gated, zero-PII, key-only, and deterministic across retries", async t => {
  configure(t);
  const timeout = t.mock.method(AbortSignal, "timeout");
  const outgoing = t.mock.method(globalThis, "fetch", async (_url: URL, _options: RequestInit) => new Response(null, { status: 202 }));
  const headers = new Headers({ Authorization: "inbound-fixture-only", Cookie: "private=fixture" });
  for (const [privacy, inbound] of [
    [{ region: "DE" }, headers], [{}, headers], [{ storedChoice: "denied", region: "US" }, headers],
    [{ storedChoice: "granted", gpc: true }, headers], [{ storedChoice: "granted" }, new Headers({ "Sec-GPC": "1" })],
  ] as const) await signalCheckoutStarted("fixture", privacy, inbound);
  assert.equal(outgoing.mock.callCount(), 0);

  for (const id of ["fixture-checkout-1", "fixture-checkout-1", "fixture-checkout-2"]) {
    await signalCheckoutStarted(id, { storedChoice: "analytics_only" }, headers);
  }
  const ids: string[] = [];
  for (const { arguments: [url, options] } of outgoing.mock.calls) {
    assert.ok(url instanceof URL);
    assert.ok(options);
    assert.equal(url.href, origin + "/v1/analytics/events:batch");
    assert.deepEqual(Object.fromEntries(new Headers(options.headers)), { "content-type": "application/json", "x-sylphx-key": key });
    assert.equal(options.method, "POST");
    assert.equal(options.redirect, "error");
    assert.equal(options.cache, "no-store");
    assert.equal(options.credentials, "omit");
    assert.equal(options.signal?.aborted, false);
    const data = JSON.parse(String(options.body));
    const id = data.events[0].event_id;
    assert.match(id, /^lockdocs-checkout-[0-9a-f]{64}$/);
    assert.deepEqual(data, { consent: "granted", events: [{ event_id: id, distinct_id: id,
      event_name: "checkout_started", properties: { entry_surface: "pricing" } }] });
    ids.push(id);
  }
  assert.equal(ids[0], ids[1]);
  assert.notEqual(ids[0], ids[2]);
  assert.deepEqual(timeout.mock.calls.map(call => call.arguments), [[10_000], [10_000], [10_000]]);
});

import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { moneyHealth } from "../src/lib/money-health";
import { GET } from "../src/routes/api/health";

const base = "https://money.example.test";
const parent = "orgs/test-org/projects/test_project/envs/test-env";
const fixture = "fixture-money-value";
const config = {
  SYLPHX_MONEY_URL: base + "/",
  SYLPHX_MONEY_PARENT: " " + parent + " ",
  SYLPHX_MONEY_API_KEY: " " + fixture + " ",
};

const previous = Object.fromEntries(Object.keys(config).map((key) => [key, process.env[key]]));
afterEach(() => {
  for (const [key, value] of Object.entries(previous)) {
    if (value === undefined) delete process.env[key]; else process.env[key] = value;
  }
});

beforeEach(async () => {
  delete process.env.SYLPHX_MONEY_API_KEY;
  await moneyHealth(); // Invalidate the process cache before each case.
  Object.assign(process.env, config);
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

async function assertHealth(money: "ok" | "error" | "unconfigured") {
  const response = await GET();
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("cache-control"), "no-store");
  assert.equal(await response.text(), JSON.stringify({ status: "ok", money }));
}

test("missing or invalid Money configuration does not fetch or gate health", async (t) => {
  const fetch = t.mock.method(globalThis, "fetch", () => { throw new Error("unexpected fetch"); });
  for (const [key, invalid] of [
    ["SYLPHX_MONEY_URL", undefined], ["SYLPHX_MONEY_URL", "http://money.example.test"],
    ["SYLPHX_MONEY_URL", base + "/path"], ["SYLPHX_MONEY_PARENT", undefined],
    ["SYLPHX_MONEY_PARENT", parent + "/extra"], ["SYLPHX_MONEY_API_KEY", undefined],
    ["SYLPHX_MONEY_API_KEY", "  "],
  ] as const) {
    Object.assign(process.env, config);
    if (invalid === undefined) delete process.env[key]; else process.env[key] = invalid;
    await assertHealth("unconfigured");
  }
  assert.equal(fetch.mock.callCount(), 0);
});

test("concurrent health requests share one read-only probe of the configured origin", async (t) => {
  const upstream = deferred<Response>();
  const fetch = t.mock.method(globalThis, "fetch", (_url: URL, _init: RequestInit) => upstream.promise);
  const requests = Array.from({ length: 10 }, () => assertHealth("ok"));
  upstream.resolve(Response.json({ env: " " + parent + " ", unrelated: fixture }));
  await Promise.all(requests);
  assert.equal(fetch.mock.callCount(), 1);
  const [url, init] = fetch.mock.calls[0].arguments;
  assert.ok(url instanceof URL && init);
  assert.equal(url.href, base + "/v1/whoami");
  assert.equal(init.method, "GET");
  assert.equal(init.body, undefined);
  assert.equal(init.redirect, "error");
  assert.equal(init.cache, "no-store");
  const headers = new Headers(init.headers);
  assert.equal(headers.get("authorization"), "Bearer " + fixture);
  assert.equal(headers.get("accept"), "application/json");
});

test("holds success for 60s and error for 15s, then recovers", async (t) => {
  let now = 123;
  t.mock.method(performance, "now", () => now);
  let status = 200;
  const fetch = t.mock.method(globalThis, "fetch", async () => Response.json({ env: parent }, { status }));
  await assertHealth("ok");
  status = 401;
  now += 59_999;
  await assertHealth("ok");
  assert.equal(fetch.mock.callCount(), 1);
  now++;
  await assertHealth("error");
  status = 200;
  now += 14_999;
  await assertHealth("error");
  assert.equal(fetch.mock.callCount(), 2);
  now++;
  await assertHealth("ok");
  assert.equal(fetch.mock.callCount(), 3);
});

test("never reuses a result or pending refresh across a configuration change", async (t) => {
  const original = deferred<Response>();
  const rotated = fixture + "-rotated";
  const fetch = t.mock.method(globalThis, "fetch", async (_url: URL, init: RequestInit) => {
    const authorization = new Headers(init.headers).get("authorization");
    if (authorization === "Bearer " + fixture) return original.promise;
    assert.equal(authorization, "Bearer " + rotated);
    return new Response(null, { status: 401 });
  });
  const pending = moneyHealth();
  process.env.SYLPHX_MONEY_API_KEY = rotated;
  await assertHealth("error");
  original.resolve(Response.json({ env: parent }));
  assert.equal(await pending, "ok");
  await assertHealth("error");
  assert.equal(fetch.mock.callCount(), 2);

  for (const [key, value] of [
    ["SYLPHX_MONEY_PARENT", parent + "-other"],
    ["SYLPHX_MONEY_URL", "https://other-money.example.test"],
  ]) {
    process.env[key] = value;
    await assertHealth("error");
  }
  assert.equal(fetch.mock.callCount(), 4);
  const [url] = fetch.mock.calls[3].arguments;
  assert.ok(url instanceof URL);
  assert.equal(url.origin, "https://other-money.example.test");
});

const failures = {
  unauthorized: () => Response.json({ env: parent, detail: fixture }, { status: 401 }),
  "server error": () => Response.json({ env: parent, detail: fixture }, { status: 500 }),
  "network error": () => { throw new TypeError(fixture); },
  "invalid JSON": () => new Response(fixture),
  "missing environment": () => Response.json({}),
  "empty environment": () => Response.json({ env: " " }),
  "non-string environment": () => Response.json({ env: 42 }),
  "wrong environment": () => Response.json({ env: parent + "-other" }),
};
for (const [failure, reply] of Object.entries(failures)) {
  test(failure + " produces only a non-gating error diagnostic", async (t) => {
    t.mock.method(globalThis, "fetch", async () => reply());
    await assertHealth("error");
  });
}

for (const stage of ["headers", "JSON body"] as const) {
  test("the 2s request deadline also applies to stalled " + stage, { timeout: 1_000 }, async (t) => {
    const controller = new AbortController();
    const deadline = t.mock.method(AbortSignal, "timeout", () => controller.signal);
    const stalled = deferred<void>();
    t.mock.method(globalThis, "fetch", async (_url: URL, { signal }: RequestInit) => {
      assert.equal(signal, controller.signal);
      const waitForAbort = (reject: (reason: unknown) => void) => {
        signal!.addEventListener("abort", () => reject(signal!.reason), { once: true });
        stalled.resolve();
      };
      if (stage === "headers") return new Promise<Response>((_, reject) => waitForAbort(reject));
      return new Response(new ReadableStream({
        pull(stream) { waitForAbort((reason) => stream.error(reason)); },
      }, { highWaterMark: 0 }));
    });
    const health = assertHealth("error");
    await stalled.promise;
    assert.deepEqual(deadline.mock.calls.map((call) => call.arguments), [[2_000]]);
    controller.abort();
    await health;
  });
}

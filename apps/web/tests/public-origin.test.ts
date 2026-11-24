import assert from "node:assert/strict";
import { test } from "node:test";
import type { APIEvent } from "@solidjs/start/server";
import { getCapabilities } from "../src/lib/auth";
import { hasPublicOrigin } from "../src/lib/public-origin";
import { GET as callback } from "../src/routes/api/auth/callback";
import { GET as oauth } from "../src/routes/api/auth/[provider]";
import { POST as email } from "../src/routes/api/auth/sign-in";
import { POST as checkout } from "../src/routes/api/billing/checkout";
import { POST as portal } from "../src/routes/api/billing/portal";

const origin = "https://lockdocs.sylphx.com";
const service = "web.fixture.svc.cluster.local";
const ingress = "http://" + service;
const headers = { host: service, "x-forwarded-host": "lockdocs.sylphx.com", "x-forwarded-proto": "https" };
const unexpected = "preview.example.test";

function event(base: string, path: string, init: RequestInit = {}) {
  return { request: new Request(base + path, init) } as APIEvent;
}

test("public target origin accepts direct HTTPS and the single Knative HTTPS forwarding pair", () => {
  for (const request of [
    new Request(origin),
    new Request(origin, { headers: { ...headers, host: "lockdocs.sylphx.com" } }),
    new Request("http://lockdocs.sylphx.com", { headers: { ...headers, host: "lockdocs.sylphx.com" } }),
    new Request(ingress, { headers }),
    new Request("https://" + service, { headers }),
    new Request(ingress + ":3000", { headers: { ...headers, host: service + ":3000", "x-forwarded-host": "Lockdocs.sylphx.com:443" } }),
  ]) assert.equal(hasPublicOrigin(request, origin), true);
});

test("public target origin refuses missing, conflicting, foreign and ambiguous authorities", () => {
  const cases: Array<[string, HeadersInit]> = [
    [ingress, {}],
    [ingress, { "x-forwarded-proto": "https", "x-forwarded-host": headers["x-forwarded-host"] }],
    [ingress, { host: service, "x-forwarded-host": headers["x-forwarded-host"] }],
    [ingress, { host: service, "x-forwarded-proto": "https" }],
    [ingress, { host: service, forwarded: "host=lockdocs.sylphx.com;proto=https", origin }],
    [ingress, { ...headers, host: service + ", " + service }],
    [ingress, { ...headers, host: service + ":3000" }],
    [ingress, { ...headers, "x-forwarded-host": "lockdocs.sylphx.com:444" }],
    [ingress, { ...headers, "x-forwarded-host": "lockdocs.sylphx.com:99999" }],
    [ingress, { ...headers, "x-forwarded-host": "lockdocs.sylphx.com@" + unexpected }],
    [ingress, { ...headers, "x-forwarded-host": "lockdocs.sylphx.com, lockdocs.sylphx.com" }],
    [ingress, { ...headers, "x-forwarded-proto": "https, https" }],
    [ingress, { ...headers, "x-forwarded-proto": "http" }],
    ["http://web.internal.local", { ...headers, host: "web.internal.local" }],
    ["https://" + unexpected, { ...headers, host: unexpected }],
    ["https://" + unexpected, headers],
    [origin, { ...headers, host: "lockdocs.sylphx.com", "x-forwarded-host": unexpected }],
    [origin, { host: unexpected }],
    ["http://lockdocs.sylphx.com", { host: "lockdocs.sylphx.com" }],
  ];
  for (const [url, input] of cases) {
    assert.equal(hasPublicOrigin(new Request(url, { headers: input }), origin), false, JSON.stringify([url, input]));
  }
});

test("proxied auth handlers use the configured public origin and preserve OAuth state and POST CSRF checks", async (t) => {
  const auth = "https://identity.example.test";
  const instance = "fixture-instance";
  const config = {
    LOCKDOCS_APP_ORIGIN: origin, SYLPHX_AUTH_URL: auth, SYLPHX_AUTH_ORGANIZATION_ID: instance,
    SYLPHX_AUTH_SECRET_KEY: "fixture-auth", SYLPHX_PUBLISHABLE_KEY: "fixture-public",
    SYLPHX_MONEY_URL: "https://billing.example.test", SYLPHX_MONEY_PARENT: "orgs/fixture/projects/fixture/envs/test",
    SYLPHX_MONEY_API_KEY: "fixture-billing", LOCKDOCS_CCR_TERMS_TEXT: "Fixture terms.", LOCKDOCS_CCR_TERMS_VERSION: "fixture",
  };
  const previous = Object.entries(config).map(([key]) => [key, process.env[key]] as const);
  Object.assign(process.env, config);
  t.after(() => {
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
  });
  let redirect!: URL;
  let authorize = "https://github.com/login/oauth/authorize?state=provider-state";
  let status = 303;
  const provider = t.mock.method(globalThis, "fetch", async (url: URL, init: RequestInit) => {
    assert.equal(url.origin, auth);
    assert.equal(url.searchParams.get("publishable_key"), config.SYLPHX_PUBLISHABLE_KEY);
    assert.equal(new Headers(init.headers).has("authorization"), false);
    if (url.pathname === "/v1/client/config") {
      assert.equal(init.redirect, "error");
      return Response.json({ instance: { id: instance }, social: [{ provider: "github", credentials: "custom" }] });
    }
    assert.equal(url.pathname, "/v1/client/oauth/github/start");
    assert.equal(url.searchParams.get("session_mode"), "server");
    assert.equal(init.redirect, "manual");
    redirect = new URL(url.searchParams.get("redirect_url")!);
    return new Response(null, { status, headers: { location: authorize } });
  });
  const startOAuth = (input: HeadersInit = headers, selected = "github") => oauth({
    ...event(ingress, "/api/auth/" + selected + "?next=%2Fpricing", { headers: input }), params: { provider: selected },
  });

  assert.equal((await getCapabilities(new Request(ingress + "/pricing", { headers })))?.github, true);
  const start = await startOAuth();
  assert.equal(start.status, 303);
  assert.equal(start.headers.get("location"), authorize);
  assert.equal(redirect.origin + redirect.pathname, origin + "/api/auth/callback");
  const cookie = start.headers.get("set-cookie")!.split(";")[0];
  const failed = await callback(event(ingress, redirect.pathname + redirect.search + "&auth_error=denied", { headers: { ...headers, cookie } }));
  assert.equal(failed.status, 303);
  assert.equal(failed.headers.get("location"), origin + "/sign-in?auth=failed&next=%2Fpricing");
  assert.deepEqual(await (await callback(event(ingress, redirect.pathname + redirect.search, { headers }))).json(), { error: "invalid_oauth_state" });

  const conflict = { ...headers, "x-forwarded-host": unexpected };
  assert.equal(await getCapabilities(new Request(ingress + "/sign-in", { headers: conflict })), null);
  assert.equal((await startOAuth(conflict)).status, 403);
  assert.equal((await startOAuth(headers, "microsoft")).status, 404);
  assert.equal((await callback(event(ingress, redirect.pathname + redirect.search, { headers: { ...conflict, cookie } }))).status, 403);

  const submit = (input: HeadersInit) => event(ingress, "/api/auth/sign-in", { method: "POST", headers: input, body: "{}" });
  assert.deepEqual(await (await email(submit({ ...headers, origin }))).json(), { error: "invalid_input" });
  for (const input of [headers, { ...headers, origin: "https://" + unexpected }, { ...conflict, origin }]) {
    const denied = await email(submit(input));
    assert.equal(denied.status, 403);
    assert.deepEqual(await denied.json(), { error: "invalid_origin" });
  }

  const buy = (input: HeadersInit) => event(ingress, "/api/billing/checkout", { method: "POST", headers: input });
  const signedOut = await checkout(buy({ ...headers, origin }));
  assert.equal(signedOut.status, 303);
  assert.equal(signedOut.headers.get("location"), origin + "/sign-in?next=%2Fpricing");
  const signedOutPortal = await portal(event(ingress, "/api/billing/portal", { method: "POST", headers: { ...headers, origin } }));
  assert.equal(signedOutPortal.status, 303);
  assert.equal(signedOutPortal.headers.get("location"), origin + "/sign-in?next=%2Fdashboard");
  assert.equal((await checkout(buy({ ...headers, origin: "https://" + unexpected }))).status, 403);
  assert.equal((await checkout(buy(headers))).status, 403);
  assert.equal(provider.mock.callCount(), 3);

  for ([status, authorize] of [
    [303, auth + "/v1/client/oauth/github/start"],
    [303, "https://accounts.google.com/o/oauth2/v2/auth"],
    [302, "https://github.com/login/oauth/authorize"],
  ] as const) {
    const retry = await startOAuth();
    assert.equal(retry.status, 303);
    assert.equal(retry.headers.get("location"), origin + "/sign-in?auth=failed&next=%2Fpricing");
    assert.equal(retry.headers.get("set-cookie"), null);
  }
});

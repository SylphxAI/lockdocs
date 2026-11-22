import assert from "node:assert/strict";
import { test } from "node:test";
import { legacySignIn, validateNext } from "../src/lib/auth-entry";
import { beginPasskey, emailSignIn, signInOptions, verifyPasskey } from "../src/lib/auth";

const origin = "https://lockdocs.example.test";
const auth = "https://auth.example.test";
const key = "pk_web_test";
const secret = "sk_web_test";
const instance = "lockdocs-auth-test";
const flow = "F".repeat(42) + "A";
const cli = `/cli/authorize?port=44191&state=${flow}&challenge=${flow}`;
const flowCookie = "__Host-lockdocs-email-flow=" + flow;
const assertion = { challenge_id: "challenge_test", credential_id: "AQ", client_data_json: "Ag", authenticator_data: "Aw", signature: "BA" };
const passkeyCookie = "__Host-lockdocs-passkey=" + assertion.challenge_id;

function post(path: string, body: object, cookie = "", requestOrigin = origin) {
  return new Request(origin + path, { method: "POST", headers: { origin: requestOrigin, cookie, "content-type": "application/json" }, body: JSON.stringify(body) });
}

test("legacy aliases keep safe product destinations without carrying untrusted state", () => {
  for (const alias of ["/login", "/signup", "/sign-up", "/register", "/forgot-password"]) {
    for (const [method, name, next] of [["GET", "next", "/pricing"], ["HEAD", "returnUrl", cli], ["GET", "nextUrl", "/pricing?currency=HKD"]]) {
      const response = legacySignIn(new Request(origin + alias + "?" + new URLSearchParams({ [name]: next, unrelated: "hidden" }), { method }))!;
      assert.equal(response.status, 308);
      assert.equal(response.headers.get("cache-control"), "private, no-store");
      assert.equal(response.headers.get("location"), "/sign-in?" + new URLSearchParams({ next }));
    }
  }
  for (const next of ["//other.example.test", "https://other.example.test", "/\\other.example.test", "/pricing?currency=USD&next=/login", "/%2fexample.test", cli.replace("44191", "65536"), cli + "&state=duplicate"]) {
    assert.equal(validateNext(next), "/dashboard", next);
  }
  const redirect = (path: string, method = "GET") => legacySignIn(new Request(origin + path, { method }))?.headers.get("location");
  assert.equal(redirect("/login/"), "/sign-in");
  assert.equal(redirect("/signup?next=%2Fpricing&next=%2Fdashboard"), "/sign-in?next=%2Fdashboard");
  assert.equal(redirect("/signup?next=%2Fpricing&returnUrl=%2Fpricing"), "/sign-in?next=%2Fdashboard");
  assert.equal(redirect("/login", "POST"), undefined);
  assert.equal(redirect("/pricing"), undefined);
});

test("headless proofs use the public credential; only a ticket redeemed for the bound instance opens a session", async t => {
  Object.assign(process.env, { LOCKDOCS_APP_ORIGIN: origin, SYLPHX_AUTH_URL: auth, SYLPHX_AUTH_ORGANIZATION_ID: instance,
    SYLPHX_PUBLISHABLE_KEY: key, SYLPHX_AUTH_SECRET_KEY: secret });
  const methods = { email_otp: false, passkey: false, password: true, sign_up: true };
  const seen: Array<{ path: string; key: string | null; bearer: string | null; body: unknown }> = [];
  const expiry = Math.floor(Date.now() / 1000) + 3600;
  let sessionInstance = instance;
  let providerError = 0;
  let configuredInstance = instance;
  t.mock.method(globalThis, "fetch", async (url: URL, init: RequestInit) => {
    const path = url.pathname.slice("/v1/client/".length);
    const body = init.body ? JSON.parse(String(init.body)) : undefined;
    const bearer = new Headers(init.headers).get("authorization");
    const publishable = url.searchParams.get("publishable_key");
    seen.push({ path, key: publishable, bearer, body });
    if (url.origin !== auth) return Response.json({}, { status: 500 });
    if (path === "tickets:redeem") return bearer === "Bearer " + secret && !publishable
      ? Response.json({ session: { token: "identity_org_session_test", expires_at: expiry } }) : Response.json({}, { status: 403 });
    if (path === "session") return bearer === "Bearer identity_org_session_test" && !publishable
      ? Response.json({ session: { id: "session", expires_at: expiry, instance_id: sessionInstance }, user: { id: "usr_test" } }) : Response.json({}, { status: 401 });
    if (bearer || publishable !== key) return Response.json({}, { status: 403 });
    if (path === "config") return Response.json({ instance: { id: configuredInstance }, methods, social: [{ provider: "google", credentials: "shared" }] });
    if (providerError) return Response.json({ private_provider_diagnostic: "not safe to reflect" }, { status: providerError });
    if (path === "continue") return Response.json({ step: "email_otp", flow, expires_in: 601 });
    if (path === "sign-in/passkey/begin") return Response.json({ challenge_id: assertion.challenge_id, challenge: flow, rp_id: "lockdocs.example.test", allowed_credential_ids: [], expires_at: expiry });
    return Response.json({ ticket: "one-time-ticket-test" });
  });

  const options = () => signInOptions(new Request(origin + "/api/auth/options", { headers: { cookie: "__Host-lockdocs-last-auth-method=google" } }));
  assert.deepEqual(await (await options()).json(), { capabilities: { github: false, google: true, email_otp: false, passkey: false, password: true }, lastUsed: "google" });
  configuredInstance = "another-instance";
  assert.equal((await options()).status, 503);
  configuredInstance = instance;
  const email = (action: string, fields = {}, cookie = "") => emailSignIn(post("/api/auth/sign-in", { action, email: "someone@example.test", ...fields }, cookie));
  assert.equal((await email("send")).status, 403);
  assert.equal((await beginPasskey(post("/api/auth/passkey/begin", {}))).status, 503);
  assert.ok(seen.every(({ path }) => path === "config"));
  const before = seen.length;
  assert.equal((await email("sign_up", { password: "new password is never accepted" })).status, 400);
  assert.equal((await emailSignIn(post("/api/auth/sign-in", { action: "send" }, "", "https://foreign.example.test"))).status, 403);
  assert.equal(seen.length, before);

  // Legacy password remains usable before Auth enables OTP or passkeys.
  const legacy = await email("password", { password: "fixture-password", next: "/pricing" });
  assert.deepEqual(await legacy.json(), { next: "/pricing" });
  assert.match(legacy.headers.get("set-cookie")!, /__Host-lockdocs-session=.*; Path=\/; Secure; HttpOnly; SameSite=Lax/);
  assert.deepEqual(seen.at(-3)?.body, { email: "someone@example.test", password: "fixture-password", session_mode: "server" });

  methods.email_otp = methods.passkey = true;
  const send = await email("send");
  assert.deepEqual(await send.json(), { step: "email_otp", expiresIn: 600 });
  assert.match(send.headers.get("set-cookie")!, /Max-Age=600; Path=\/; Secure; HttpOnly; SameSite=Lax/);
  assert.deepEqual(seen.at(-1)?.body, { email: "someone@example.test", method: "email_otp", session_mode: "server" });
  assert.equal((await email("verify", { code: "012345" })).status, 400);
  assert.equal((await email("verify", { code: "12345" }, flowCookie)).status, 400);
  providerError = 429;
  assert.deepEqual(await (await email("send")).json(), { error: "try_later" });
  providerError = 401;
  const bad = await email("verify", { code: "999999" }, flowCookie);
  assert.equal(bad.status, 401);
  assert.deepEqual(await bad.json(), { error: "auth_failed" });
  assert.equal(bad.headers.get("set-cookie"), null);
  providerError = 0;
  const verified = await email("verify", { code: "012345", next: "//foreign.example.test" }, flowCookie);
  assert.deepEqual(await verified.json(), { next: "/dashboard" });
  assert.deepEqual(seen.at(-3)?.body, { flow, code: "012345" });
  assert.deepEqual(seen.at(-2)?.body, { ticket: "one-time-ticket-test" });
  assert.equal(seen.at(-2)?.bearer, "Bearer " + secret);

  const begin = await beginPasskey(post("/api/auth/passkey/begin", {}));
  assert.deepEqual(await begin.json(), { challenge_id: assertion.challenge_id, challenge: flow, rp_id: "lockdocs.example.test", allowed_credential_ids: [] });
  assert.deepEqual(seen.at(-1)?.body, { rp_id: "lockdocs.example.test", origin });
  assert.equal((await verifyPasskey(post("/api/auth/passkey/verify", assertion, "__Host-lockdocs-passkey=different"))).status, 400);
  sessionInstance = "another-instance";
  const wrongInstance = await verifyPasskey(post("/api/auth/passkey/verify", assertion, passkeyCookie));
  assert.equal(wrongInstance.status, 401);
  assert.ok(!wrongInstance.headers.get("set-cookie")?.includes("__Host-lockdocs-session="));
  sessionInstance = instance;
  const passkey = await verifyPasskey(post("/api/auth/passkey/verify", { ...assertion, next: cli }, passkeyCookie));
  assert.deepEqual(await passkey.json(), { next: cli });
  assert.deepEqual(seen.at(-3)?.body, { ...assertion, session_mode: "server" });
  assert.ok(seen.filter(entry => !["session", "tickets:redeem"].includes(entry.path)).every(entry => entry.key === key && !entry.bearer));
  assert.ok(!seen.some(entry => entry.path === "sign-up"));
});

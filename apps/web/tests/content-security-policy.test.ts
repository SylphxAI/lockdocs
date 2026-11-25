import assert from "node:assert/strict";
import { test } from "node:test";
import { documentPolicy } from "../src/lib/content-security-policy";

const api = "https://api.sylphx.com";
const tenant = "https://auth.lockdocs.sylphx.com";
const signal = "https://signal.example.test";
const directives = (development: boolean) => Object.fromEntries(documentPolicy("fixture", development).split("; ").map(directive => {
  const [name, ...sources] = directive.split(" ");
  assert.equal(new Set(sources).size, sources.length, name + " contains a duplicate source");
  return [name, sources];
}));

test("CSP deduplicates overlapping origins without removing tenant auth or native CLI loopback", t => {
  const env = { SYLPHX_AUTH_URL: api, LOCKDOCS_TENANT_AUTH_ORIGIN: tenant, SYLPHX_API_URL: api, SYLPHX_PUBLISHABLE_KEY: "sylphx_pk_signal_fixture" };
  for (const [name, value] of Object.entries(env)) {
    const prior = process.env[name];
    t.after(() => { if (prior === undefined) delete process.env[name]; else process.env[name] = prior; });
    process.env[name] = value;
  }
  const policy = directives(false);
  assert.equal(policy["connect-src"].filter(source => source === api).length, 1);
  assert.ok(policy["connect-src"].includes(tenant));
  assert.ok(policy["connect-src"].includes("https://www.google-analytics.com"));
  assert.ok(!policy["connect-src"].some(source => source.startsWith("ws:")));
  assert.deepEqual(policy["form-action"], ["'self'", "https://checkout.stripe.com", "https://billing.stripe.com", "http://127.0.0.1:*", api, tenant]);

  process.env.LOCKDOCS_TENANT_AUTH_ORIGIN = api + "/";
  process.env.SYLPHX_API_URL = signal;
  const dev = directives(true);
  for (const source of [api, signal, "ws://localhost:*", "ws://127.0.0.1:*"]) assert.ok(dev["connect-src"].includes(source));
  assert.ok(!dev["form-action"].includes(signal));
});

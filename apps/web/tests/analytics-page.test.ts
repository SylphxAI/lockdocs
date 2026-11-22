import { strict as assert } from "node:assert";
import { test } from "node:test";
import { analyticsPage } from "../src/lib/analytics-page";

const origin = "https://lockdocs.sylphx.com";

test("reports the actual Lockdocs pricing URL and a scrubbed same-origin referrer", () => {
  assert.deepEqual(analyticsPage(origin + "/pricing?code=example&email=fixture%40example.test#access_token=example", origin + "/docs?token=example", origin), {
    page_location: "https://lockdocs.sylphx.com/pricing",
    page_title: "Pricing · Lockdocs",
    page_referrer: "https://lockdocs.sylphx.com/docs",
  });
});

test("refuses other origins, credentials, the tag endpoint and unknown or user-defined paths", () => {
  for (const url of [
    "https://example.test/pricing", "https://lockdocs.sylphx.com.example.test/pricing",
    "https://user:secret@lockdocs.sylphx.com/pricing", "blob:" + origin + "/pricing",
    origin + "/api/analytics/tag", origin + "/docs/fixture%40example.test", "not-a-url",
  ]) assert.equal(analyticsPage(url, "", origin), null, url);
  for (const referrer of ["https://example.test/?email=fixture", origin + "/unknown/secret"]) {
    assert.equal(analyticsPage(origin + "/pricing", referrer, origin)?.page_referrer, "");
  }
});

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { withCampaign } from "../src/lib/campaign-navigation";

test("keeps campaign values separate from currency, anchors and auth return paths", () => {
  const touch = "gclid=g.1&gbraid=g-2&wbraid=w_3&utm_id=123456&utm_source=paid&utm_medium=cpc&utm_campaign=C%2B%2B+%26+Rust&utm_term=lockfile&utm_content=docs";
  assert.equal(withCampaign("/pricing?currency=EUR#plans", "?" + touch + "&email=private"), "/pricing?currency=EUR&" + touch + "#plans");
  assert.equal(withCampaign("/sign-in?next=%2Fpricing", "?" + touch), "/sign-in?next=%2Fpricing&" + touch);
});

test("does not add duplicated, invalid, unknown or external campaign parameters", () => {
  const touch = "?gclid=g&gclid=duplicate&gbraid=a%2Fb&utm_id=one&utm_id=two&utm_campaign=%0A&utm_term=one&utm_term=two&next=%2Fdashboard&fbclid=f";
  assert.equal(withCampaign("/pricing?currency=EUR", touch), "/pricing?currency=EUR");
  for (const path of ["https://elsewhere.example/", "//elsewhere.example", "/\\elsewhere.example", "#plans"]) {
    assert.equal(withCampaign(path, "?gclid=test"), path);
  }
  assert.equal(withCampaign("/pricing?utm_source=direct", "?utm_source=paid"), "/pricing?utm_source=direct");
});

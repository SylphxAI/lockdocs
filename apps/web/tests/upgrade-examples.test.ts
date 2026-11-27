import assert from "node:assert/strict";
import { test } from "node:test";
import { releaseSourceUrl, upgrades } from "../src/lib/upgrade-examples";

test("the pricing decision proof links both Next.js citations to their published files", () => {
  assert.equal(releaseSourceUrl(upgrades.next.oldSource), "https://unpkg.com/next%4014.2.3/dist/client/components/headers.d.ts");
  assert.equal(releaseSourceUrl(upgrades.next.newSource), "https://unpkg.com/next%4015.0.0/dist/server/request/cookies.d.ts");
  assert.equal(releaseSourceUrl("@acme/session@1.2.0 dist/index.d.ts:4"), "https://unpkg.com/@acme/session%401.2.0/dist/index.d.ts");
});

test("citations without exactly one file and line get no link", () => {
  assert.equal(releaseSourceUrl(upgrades.react.newSource), undefined);
  assert.equal(releaseSourceUrl("next dist/server/request/cookies.d.ts:24"), undefined);
  assert.equal(releaseSourceUrl("next@15.0.0 dist/server/request/cookies.d.ts"), undefined);
});

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { allowIndexing } from "../src/lib/indexing";

const production = "lockdocs.sylphx.com";
const knative = "web.mad-coal-42in6b.svc.cluster.local";
const preview = "preview.lockdocs.sylphx.app";

function isIndexable(host: string, forwarded?: string) {
  const headers = new Headers({ Host: host });
  if (forwarded !== undefined) headers.set("X-Forwarded-Host", forwarded);
  return allowIndexing(headers);
}

test("direct production and Knative's forwarded production authority are indexable", () => {
  assert.equal(isIndexable("Lockdocs.sylphx.com:443"), true);
  assert.equal(isIndexable(knative, "Lockdocs.sylphx.com:443"), true);
});

test("preview, unrecognized or ambiguous authorities are not indexable", () => {
  for (const [host, forwarded] of [
    [preview], [knative], [knative, preview], [production, preview],
    [preview, production], ["unknown.example", production],
    [knative, production + ":443, " + preview], [knative, production + ", " + production],
    [knative, production + ".example"], [knative, ""],
  ]) {
    assert.equal(isIndexable(host, forwarded), false, [host, forwarded].join(" / "));
  }
});

import assert from "node:assert/strict";
import { test } from "node:test";
import { legacyAlias } from "../src/lib/legacy-aliases";

const origin = "https://lockdocs.example.test";
const alias = (path: string, method = "GET") => legacyAlias(new Request(origin + path, { method }));

test("legacy aliases permanently redirect page loads straight to the canonical page", () => {
  for (const [path, location, cache] of [["/guide/quickstart", "/guide", null], ["/reference/cli", "/reference", null], ["/recover", "/dashboard", "private, no-store"]] as const) {
    for (const method of ["GET", "HEAD"]) {
      for (const variant of [path, path + "/", path + "?session_id=cs_test&token=secret&utm_source=x&gclid=test-id"]) {
        const response = alias(variant, method)!;
        assert.equal(response.status, 308, method + " " + variant);
        assert.equal(response.headers.get("location"), location, variant);
        assert.equal(response.headers.get("cache-control"), cache, variant);
      }
    }
    for (const method of ["POST", "PUT", "OPTIONS"]) assert.equal(alias(path, method), undefined, method + " " + path);
  }
});

test("only the exact legacy paths are aliases", () => {
  for (const path of ["/guide", "/reference", "/dashboard", "/quickstart", "/recover//", "/guide/quickstart//", "/recover/x", "/zh-TW/recover", "/Recover", "/recover.html", "/reference/cli-x", "/"]) {
    assert.equal(alias(path), undefined, path);
  }
});

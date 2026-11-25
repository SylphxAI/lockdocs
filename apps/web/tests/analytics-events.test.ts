import assert from "node:assert/strict";
import { test } from "node:test";
import { articleSurface, behavioralParams } from "../src/lib/analytics-events";

test("behavioral events project only their allowlisted scalar fields", () => {
  assert.deepEqual(behavioralParams("lockfile_resolved", {
    lockfile_format: "uv.lock", source_kind: "pasted", package_count: 0,
    input: "private dependency data", package_name: "private-package",
  }), { lockfile_format: "uv.lock", source_kind: "pasted", package_count: 0 });
  assert.deepEqual(behavioralParams("cli_authorized", {
    entry_surface: "cli_authorize", state: "fixture-state", port: 1234, code: "fixture-code",
  }), { entry_surface: "cli_authorize" });
});

test("Playground copy events never forward command text or package names", () => {
  for (const surface of ["playground_cli", "playground_upgrade"]) {
    assert.deepEqual(behavioralParams("command_copied", {
      surface, command_kind: "cli", command: "lockdocs internal-payroll-fixture --offline", package_name: "internal-payroll-fixture",
    }), { surface, command_kind: "cli" });
  }
  assert.equal(behavioralParams("command_copied", { surface: "playground_unknown", command_kind: "cli" }), null);
});

test("the editorial Pro CTA reports only its named surface", () => {
  assert.deepEqual(behavioralParams("pro_cta_clicked", {
    entry_surface: "context7_alternative", gclid: "TEST999", email: "fixture@example.test",
  }), { entry_surface: "context7_alternative" });
  assert.equal(behavioralParams("paywall_view", { entry_surface: "context7_alternative" }), null);
});

test("only allowlisted article surfaces can report Pro clicks, not paywall impressions", () => {
  for (const [route, entry_surface] of [
    ["/compare", "article_compare"], ["/benchmarks", "article_benchmarks"],
    ["/guide", "article_guide"], ["/pro", "article_pro"],
    ["/guide/setup", "article_guide_setup"], ["/guide/ecosystems", "article_guide_ecosystems"],
    ["/guide/fetch", "article_guide_fetch"], ["/guide/how-it-works", "article_guide_how_it_works"],
    ["/reference", "article_reference"], ["/reference/tools", "article_reference_tools"],
    ["/capabilities", "article_capabilities"], ["/docs", "article_docs"], ["/zh-TW/docs", "article_docs_zh_tw"],
  ]) {
    assert.equal(articleSurface(route), entry_surface);
    assert.deepEqual(behavioralParams("pro_cta_clicked", { entry_surface, token: "private-sentinel" }), { entry_surface });
    assert.equal(behavioralParams("paywall_view", { entry_surface }), null);
  }
  for (const route of ["/vision", "/legal/privacy", "/zh-TW/guide", "/pro?token=private", "constructor"]) {
    assert.equal(articleSurface(route), undefined);
    assert.equal(behavioralParams("pro_cta_clicked", { entry_surface: route }), null);
  }
});

test("behavioral events refuse unknown names and missing or invalid values", () => {
  assert.equal(behavioralParams("toString", {}), null);
  assert.equal(behavioralParams("lockfile_resolved", { lockfile_format: "Cargo.lock", source_kind: "sample" }), null);
  for (const package_count of [-1, "3", 3.5, Infinity]) {
    assert.equal(behavioralParams("lockfile_resolved", { lockfile_format: "Cargo.lock", source_kind: "sample", package_count }), null);
  }
  assert.equal(behavioralParams("command_copied", { surface: "docs_code", command_kind: "fixture@example.test" }), null);
  assert.equal(behavioralParams("paywall_view", { entry_surface: "cli_authorize" }), null);
  assert.equal(behavioralParams("upgrade_preview_switched", { example_id: "private-package" }), null);
});

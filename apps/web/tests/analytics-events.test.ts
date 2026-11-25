import assert from "node:assert/strict";
import { test } from "node:test";
import { behavioralParams } from "../src/lib/analytics-events";

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

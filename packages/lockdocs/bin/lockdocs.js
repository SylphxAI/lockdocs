#!/usr/bin/env node
// lockdocs launcher: runs the native binary for this platform.
"use strict";
const { spawnSync } = require("node:child_process");
const { existsSync } = require("node:fs");
const path = require("node:path");

const PLATFORMS = {
  "darwin-arm64": "@sylphx/lockdocs-darwin-arm64",
  "darwin-x64": "@sylphx/lockdocs-darwin-x64",
  "linux-x64": "@sylphx/lockdocs-linux-x64-gnu",
  "linux-arm64": "@sylphx/lockdocs-linux-arm64-gnu",
  "win32-x64": "@sylphx/lockdocs-win32-x64-msvc",
};

function resolveBinary() {
  if (process.env.LOCKDOCS_BIN && existsSync(process.env.LOCKDOCS_BIN)) return process.env.LOCKDOCS_BIN;
  const exe = process.platform === "win32" ? "lockdocs.exe" : "lockdocs";
  const pkg = PLATFORMS[`${process.platform}-${process.arch}`];
  if (pkg) {
    try {
      return require.resolve(`${pkg}/${exe}`);
    } catch {}
  }
  // Development checkout: packages/lockdocs/bin -> repo root target/.
  const root = path.resolve(__dirname, "..", "..", "..");
  for (const p of [path.join(root, "target", "release", exe), path.join(root, "target", "debug", exe)]) {
    if (existsSync(p)) return p;
  }
  return null;
}

const bin = resolveBinary();
if (!bin) {
  console.error(
    `lockdocs: no native binary for ${process.platform}-${process.arch}.\n` +
      "Supported: macOS (arm64, x64), Linux glibc (x64, arm64), Windows x64.\n" +
      "If optional dependencies were skipped, reinstall without --no-optional, or build from source: cargo install --git https://github.com/SylphxAI/lockdocs lockdocs"
  );
  process.exit(1);
}
const res = spawnSync(bin, process.argv.slice(2), { stdio: "inherit", windowsHide: true });
if (res.error) {
  console.error(`lockdocs: failed to start ${bin}: ${res.error.message}`);
  process.exit(1);
}
process.exit(res.status === null ? 1 : res.status);

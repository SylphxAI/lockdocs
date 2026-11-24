// The offline check runs in CI. --preflight is read-only and runs after npm publishes.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const args = process.argv.slice(2);
assert(args.length === 0 || (args.length === 1 && args[0] === "--preflight"),
  "Usage: bun scripts/check-mcp-registry.ts [--preflight]");

const read = (path: string) => JSON.parse(readFileSync(path, "utf8"));
const pkg = read("packages/lockdocs/package.json");
const server = read("server.json");
const cursor = read("mcp.json").mcpServers.lockdocs;
const repository = pkg.repository.url.replace(/^git\+/, "").replace(/\.git$/, "");
const ownerAndRepo = new URL(repository).pathname.slice(1);
assert.equal(pkg.mcpName, "io.github." + ownerAndRepo, "Keep the case of the published MCP name");
assert.equal(server.name, pkg.mcpName, "Registry name must match npm mcpName");
assert.equal(server.version, pkg.version, "Registry version must match npm version");
assert.equal(server.description, pkg.description, "Registry description must match npm description");
assert.equal(server.websiteUrl, pkg.homepage, "Registry website must match npm homepage");
assert.deepEqual(server.repository, { url: repository, source: "github" });
assert.equal(cursor.command, "npx");
assert.deepEqual(cursor.args, ["-y", pkg.name, "mcp"], "Expected the local stdio launcher");
assert.deepEqual(server.packages, [{
  registryType: "npm",
  runtimeHint: cursor.command,
  identifier: pkg.name,
  version: pkg.version,
  transport: { type: "stdio" },
  packageArguments: [{ type: "positional", value: "mcp" }],
}], "Expected a single npm package with the local stdio launcher");
assert.equal(server.remotes, undefined, "Lockdocs does not offer a hosted MCP endpoint");
console.log("Registry identity and stdio launch match " + pkg.name + " " + pkg.version);

if (args[0] === "--preflight") {
  const npmUrl = "https://registry.npmjs.org/" + encodeURIComponent(pkg.name) + "/" + encodeURIComponent(pkg.version);
  const published = await fetch(npmUrl, { signal: AbortSignal.timeout(15_000) });
  assert(published.ok, "Publish the exact npm version first (HTTP " + published.status + ")");
  const metadata = await published.json();
  for (const key of ["name", "version", "mcpName", "bin"]) {
    assert.deepEqual(metadata[key], pkg[key], "Published npm metadata differs: " + key);
  }

  const registryUrl = "https://registry.modelcontextprotocol.io/v0.1/servers/" +
    encodeURIComponent(server.name) + "/versions/" + encodeURIComponent(server.version);
  const existing = await fetch(registryUrl, { signal: AbortSignal.timeout(15_000), redirect: "error" });
  if (existing.ok) throw new Error("This version is already registered. Do not publish it again: " + registryUrl);
  assert.equal(existing.status, 404, "Could not establish that the Registry version is absent");
  console.log("This npm release is published and this Registry version is not listed yet.");
}

// Fails when any manifest disagrees with packages/lockdocs/package.json.
import { readFileSync, readdirSync } from "node:fs";

const read = (p: string) => JSON.parse(readFileSync(p, "utf8"));
const v = read("packages/lockdocs/package.json").version;
const bad: string[] = [];
const eq = (what: string, got: string) => got !== v && bad.push(`${what}: ${got}`);
for (const [k, x] of Object.entries(read("packages/lockdocs/package.json").optionalDependencies)) eq(k, x as string);
for (const d of readdirSync("packages/npm")) eq(`packages/npm/${d}`, read(`packages/npm/${d}/package.json`).version);
const s = read("server.json");
eq("server.json", s.version);
eq("server.json package", s.packages[0].version);
eq("package.json", read("package.json").version);
const cargo = readFileSync("Cargo.toml", "utf8").match(/\[workspace\.package\][^\[]*?version = "([^"]+)"/)?.[1] ?? "";
eq("Cargo.toml", cargo);
const lock = readFileSync("Cargo.lock", "utf8").match(/name = "lockdocs"\nversion = "([^"]+)"/)?.[1] ?? "";
eq("Cargo.lock", lock);
if (bad.length) {
  console.error(`version mismatch (want ${v}):\n  ${bad.join("\n  ")}`);
  process.exit(1);
}
console.log(`all manifests at ${v}`);

// Usage: bun scripts/set-version.ts 1.2.3 — sets one version everywhere.
import { readFileSync, writeFileSync, readdirSync } from "node:fs";

const v = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(v ?? "")) throw new Error("usage: bun scripts/set-version.ts <semver>");

const json = (p: string, f: (x: any) => void) => {
  const x = JSON.parse(readFileSync(p, "utf8"));
  f(x);
  writeFileSync(p, JSON.stringify(x, null, 2) + "\n");
};
json("packages/lockdocs/package.json", (p) => {
  p.version = v;
  for (const k of Object.keys(p.optionalDependencies)) p.optionalDependencies[k] = v;
});
for (const d of readdirSync("packages/npm")) json(`packages/npm/${d}/package.json`, (p) => (p.version = v));
json("server.json", (s) => {
  s.version = v;
  s.packages[0].version = v;
});
json("package.json", (p) => (p.version = v));
const cargo = readFileSync("Cargo.toml", "utf8").replace(/(\[workspace\.package\][^\[]*?version = ")[^"]+(")/, `$1${v}$2`);
writeFileSync("Cargo.toml", cargo);
console.log(`version set to ${v}; run cargo update -w to refresh Cargo.lock`);

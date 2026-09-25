// One tagline everywhere. Source: packages/lockdocs/package.json "description".
// Checks the README lead, the docs hero and meta description, server.json and
// the crate description; CI also compares the GitHub repository description.
import { readFileSync } from "node:fs";

const t: string = JSON.parse(readFileSync("packages/lockdocs/package.json", "utf8")).description;
const bad: string[] = [];
const has = (file: string, needle: string) => readFileSync(file, "utf8").includes(needle) || bad.push(`${file} lacks: ${needle}`);
has("README.md", `**${t}**`);
has("docs/index.md", `tagline: ${t}`);
has("docs/.vitepress/config.ts", `const desc = '${t}'`);
has("crates/lockdocs/Cargo.toml", `description = "${t}"`);
if (JSON.parse(readFileSync("server.json", "utf8")).description !== t) bad.push("server.json description differs");
const gh = process.argv[2];
if (gh !== undefined && gh.trim() !== t) bad.push(`GitHub description differs: ${gh.trim()}`);
if (bad.length) {
  console.error(`tagline mismatch (want "${t}"):\n  ${bad.join("\n  ")}`);
  process.exit(1);
}
console.log(`tagline consistent: ${t}`);

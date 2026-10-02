// The README is the npm landing page: npm resolves relative URLs against packages/lockdocs, so they 404.
import { readFileSync } from "node:fs";

const bad: string[] = [];
readFileSync("README.md", "utf8").split("\n").forEach((line, i) => {
  for (const m of line.matchAll(/(?:src|href)="([^"]*)"|\]\(([^)\s]*)/g)) {
    const url = m[1] ?? m[2] ?? "";
    if (url && !/^(https?:|#|mailto:)/.test(url)) bad.push(`README.md:${i + 1} relative link ${url}`);
  }
});
if (bad.length) {
  console.error(`${bad.join("\n")}\nUse absolute URLs in README.md.`);
  process.exit(1);
}
console.log("README links are absolute");

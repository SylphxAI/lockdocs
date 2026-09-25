// docs/capabilities.md: every path a supported or partial row names exists,
// and planned or retired rows name none.
import { existsSync, readFileSync } from "node:fs";

const rows = readFileSync("docs/capabilities.md", "utf8")
  .split("\n")
  .filter((l) => l.startsWith("| LD-"))
  .map((l) => l.split("|").slice(1, -1).map((c) => c.trim()));
const bad: string[] = [];
const statuses = ["supported", "partial", "planned", "retired"];
for (const [id, , status, code] of rows) {
  const paths = code ? code.split(",").map((p) => p.trim()).filter(Boolean) : [];
  if (!statuses.includes(status) && !status.startsWith("rename-to:")) bad.push(`${id}: unknown status ${status}`);
  if (status === "supported" || status === "partial") {
    if (!paths.length) bad.push(`${id}: ${status} but names no code`);
    for (const p of paths) if (!existsSync(p)) bad.push(`${id}: ${p} does not exist`);
  } else if (paths.length) bad.push(`${id}: ${status} but names code (${paths.join(", ")})`);
}
if (!rows.length) bad.push("no capability rows found");
if (bad.length) {
  console.error(`docs/capabilities.md:\n  ${bad.join("\n  ")}`);
  process.exit(1);
}
console.log(`capabilities: ${rows.length} rows, every named path exists`);

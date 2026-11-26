import type { Dependency } from "./playground";
import type { upgrades } from "./upgrade-examples";

const cliEcosystems = { npm: "npm", crate: "cargo", PyPI: "pypi", Go: "go" } satisfies Record<Dependency["ecosystem"], string>;

export function offlinePackageCommand(row: Dependency) {
  const spec = `${cliEcosystems[row.ecosystem]}:${row.name}@${row.version}`;
  // The lockfile parser limits these characters; quote PyPI epochs for shell history expansion.
  return `npx -y @sylphx/lockdocs ${spec.includes("!") ? `'${spec}'` : spec} --offline`;
}

export function filterPackages(rows: readonly Dependency[], query: string) {
  const term = query.trim().toLowerCase();
  return term ? rows.filter(row => [row.name, row.version, row.ecosystem].some(field => field.toLowerCase().includes(term))) : rows;
}

export function matchingUpgrade(rows: readonly Dependency[]): keyof typeof upgrades | undefined {
  if (rows.some(row => row.ecosystem === "npm" && row.name === "next")) return "next";
  if (rows.some(row => row.ecosystem === "PyPI" && row.name === "pydantic")) return "pydantic";
  if (rows.some(row => row.ecosystem === "npm" && row.name === "zod")) return "zod";
  if (rows.some(row => row.ecosystem === "npm" && row.name === "react")) return "react";
}

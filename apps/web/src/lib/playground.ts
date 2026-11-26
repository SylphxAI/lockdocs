import { parse as jsonc, type ParseError } from "jsonc-parser";
import { parse as toml } from "smol-toml";

import type { Format } from "./lockfile-formats";
import { previewLimit } from "./playground-file";
import { upgrades } from "./upgrade-examples";
import { readGoMod, readPnpm, readYarn } from "./playground-lockfiles";
export type Dependency = { name: string; version: string; ecosystem: "npm" | "crate" | "PyPI" | "Go"; source: "release" | "git" };
const record = (value: unknown): Record<string, unknown> => value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};

export function resolveLockfile(format: Format, input: string) {
  if (input.length > previewLimit) throw new Error("This preview accepts up to 400,000 characters. Run the CLI for larger lockfiles.");
  const rows = new Map<string, Dependency>();
  const add = (name: unknown, version: unknown, ecosystem: Dependency["ecosystem"], git = false) => {
    if (typeof name !== "string" || typeof version !== "string" || version.length > 120) return;
    if (ecosystem === "Go") {
      if (name.length > 400 || !/^[A-Za-z0-9_~-][A-Za-z0-9_.~-]*(?:\/[A-Za-z0-9_~-][A-Za-z0-9_.~-]*)*$/.test(name) ||
        !/^v\d+\.\d+\.\d+(?:-[\w.-]+)?(?:\+[\w.-]+)?$/.test(version)) return;
    } else if (!/^(?:@[A-Za-z0-9_.-]+\/)?[A-Za-z0-9_.-]{1,214}$/.test(name) ||
      !/^[0-9][A-Za-z0-9_.+!-]{0,119}$/.test(version)) return;
    if (ecosystem === "PyPI") name = name.toLowerCase().replace(/[-_.]+/g, "-");
    const row = { name: name as string, version, ecosystem, source: git ? "git" as const : "release" as const };
    rows.set(JSON.stringify(row), row);
  };
  if (format === "pnpm-lock.yaml") readPnpm(input, add);
  else if (format === "yarn.lock") readYarn(input, add);
  else if (format === "go.mod") readGoMod(input, add);
  else if (format === "requirements.txt") {
    for (const line of input.split(/\r?\n/)) {
      const m = line.trim().match(/^([\w.-]+)(?:\[[\w,.-]+\])?\s*==\s*([\w.+!-]+)(?:\s|;|$)/);
      if (m) add(m[1], m[2], "PyPI");
    }
  } else if (format === "Cargo.lock" || format === "uv.lock") {
    const value = toml(input);
    if (Array.isArray(value.package)) for (const item of value.package) {
      const row = record(item);
      const source = record(row.source);
      if (format === "Cargo.lock" && typeof row.source === "string") add(row.name, row.version, "crate", row.source.startsWith("git+"));
      else if (format === "uv.lock" && (source.registry || source.git)) add(row.name, row.version, "PyPI", Boolean(source.git));
    }
  } else {
    const errors: ParseError[] = [];
    const data = format === "bun.lock" ? jsonc(input, errors, { allowTrailingComma: true }) : JSON.parse(input);
    if (errors.length || !data || typeof data !== "object" || Array.isArray(data)) throw new Error("Invalid lockfile");
    if (format === "bun.lock") {
      for (const [key, entry] of Object.entries(record(data.packages))) {
        if (key.split("/").length > (key.startsWith("@") ? 2 : 1) || !Array.isArray(entry) || typeof entry[0] !== "string") continue;
        const split = entry[0].lastIndexOf("@");
        if (split > 0) add(entry[0].slice(0, split), entry[0].slice(split + 1), "npm");
      }
    } else if (data.packages) {
      for (const [path, value] of Object.entries(record(data.packages))) {
        const name = path.match(/^node_modules\/((?:@[^/]+\/)?[^/]+)$/)?.[1];
        const row = record(value);
        if (name && !row.link) add(row.name ?? name, row.version, "npm", typeof row.resolved === "string" && /^(?:git[+:]|ssh:)/.test(row.resolved));
      }
    } else {
      for (const [name, value] of Object.entries(record(data.dependencies))) {
        const row = record(value);
        if (!row.link) add(name, row.version, "npm", typeof row.resolved === "string" && /^(?:git[+:]|ssh:)/.test(row.resolved));
      }
    }
  }
  return [...rows.values()].sort((a, b) => JSON.stringify(a) < JSON.stringify(b) ? -1 : 1);
}

export function sampleCitation(row: Dependency) {
  if (row.source !== "release") return null;
  if (row.name === "next" && row.ecosystem === "npm") return row.version === "15.0.0" ? upgrades.next.newSource : row.version === "14.2.3" ? upgrades.next.oldSource : null;
  if (row.name === "pydantic" && row.ecosystem === "PyPI") return row.version === "2.9.2" ? upgrades.pydantic.newSource : row.version === "1.10.18" ? upgrades.pydantic.oldSource : null;
  return null;
}

// Source excerpts are pinned to the release represented by each sample.
export function sampleExcerpt(row: Dependency) {
  if (row.source !== "release" || row.ecosystem !== "npm") return null;
  if (row.name === "react" && row.version === "18.3.1") return {
    cite: "react@18.3.1 packages/react/src/ReactHooks.js:80–85",
    url: "https://github.com/facebook/react/blob/v18.3.1/packages/react/src/ReactHooks.js#L80-L85",
    code: "export function useState<S>(\n  initialState: (() => S) | S,\n): [S, Dispatch<BasicStateAction<S>>] {\n  const dispatcher = resolveDispatcher();\n  return dispatcher.useState(initialState);\n}",
  };
  if (row.name === "zod" && row.version === "4.6.7") return {
    cite: "zod@4.6.7 packages/zod/src/v4/classic/schemas.ts:710–712",
    url: "https://github.com/colinhacks/zod/blob/v4.6.7/packages/zod/src/v4/classic/schemas.ts#L710-L712",
    code: "export function email(params?: string | core.$ZodEmailParams): ZodEmail {\n  return core._email(ZodEmail, params);\n}",
  };
  return null;
}

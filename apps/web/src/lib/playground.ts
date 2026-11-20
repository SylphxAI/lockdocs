import { parse as jsonc, type ParseError } from "jsonc-parser";
import { parse as toml } from "smol-toml";

export const formats = ["package-lock.json", "bun.lock", "Cargo.lock", "uv.lock", "requirements.txt"] as const;
export type Format = (typeof formats)[number];
export type Dependency = { name: string; version: string; ecosystem: "npm" | "crate" | "PyPI"; source: "release" | "git" };
const record = (value: unknown): Record<string, unknown> => value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};

export function resolveLockfile(format: Format, input: string) {
  if (input.length > 400_000) throw new Error("This preview accepts up to 400,000 characters. Run the CLI for larger lockfiles.");
  const rows = new Map<string, Dependency>();
  const add = (name: unknown, version: unknown, ecosystem: Dependency["ecosystem"], git = false) => {
    if (typeof name !== "string" || !/^(?:@[A-Za-z0-9_.-]+\/)?[A-Za-z0-9_.-]{1,214}$/.test(name) ||
      typeof version !== "string" || !/^[0-9][A-Za-z0-9_.+!-]{0,119}$/.test(version)) return;
    if (ecosystem === "PyPI") name = name.toLowerCase().replace(/[-_.]+/g, "-");
    const row = { name: name as string, version, ecosystem, source: git ? "git" as const : "release" as const };
    rows.set(JSON.stringify(row), row);
  };
  if (format === "requirements.txt") {
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

export const samples: Record<Format, string> = {
  "package-lock.json": JSON.stringify({ name: "my-next-app", lockfileVersion: 3, packages: {
    "": { dependencies: { next: "15.0.0", react: "18.3.1", zod: "4.6.7" } },
    "node_modules/next": { version: "15.0.0" }, "node_modules/react": { version: "18.3.1" }, "node_modules/zod": { version: "4.6.7" },
  } }, null, 2),
  "bun.lock": "{\n  \"lockfileVersion\": 1,\n  \"workspaces\": {\"\": {\"dependencies\": {\"next\": \"14.2.3\"}}},\n  \"packages\": {\n    \"next\": [\"next@14.2.3\", \"\", {}],\n    \"zod\": [\"zod@4.6.7\", \"\", {}],\n  },\n}",
  "Cargo.lock": "version = 4\n\n[[package]]\nname = \"tokio\"\nversion = \"1.44.2\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.219\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"",
  "uv.lock": "version = 1\n\n[[package]]\nname = \"pydantic\"\nversion = \"2.9.2\"\nsource = { registry = \"https://pypi.org/simple\" }\n\n[[package]]\nname = \"annotated-types\"\nversion = \"0.7.0\"\nsource = { registry = \"https://pypi.org/simple\" }",
  "requirements.txt": "# Example project\npydantic==1.10.18\nrequests==2.32.3\n",
};

export const upgrades = {
  next: {
    label: "Next.js 14 → 15", command: "lockdocs upgrade next 15.0.0", change: "cookies() is now asynchronous",
    path: "app/account/page.tsx:8", before: 'const token = cookies().get("session")', after: 'const token = (await cookies()).get("session")',
    oldSource: "next@14.2.3 dist/client/components/headers.d.ts:13", newSource: "next@15.0.0 dist/server/request/cookies.d.ts:24",
    links: [["Next.js 15 migration · Async request APIs", "https://nextjs.org/docs/app/guides/upgrading/version-15#async-request-apis-breaking-change"]],
    note: "Make the containing Server Component async. Next.js 15 provides temporary synchronous compatibility, but the declared return value is now a Promise.",
  },
  pydantic: {
    label: "Pydantic 1.10 → 2.9", command: "lockdocs upgrade pydantic 2.9.2", change: "Use model_dump() in Pydantic v2",
    path: "app/serializers.py:14", before: "payload = account.dict()", after: "payload = account.model_dump()",
    oldSource: "pydantic@1.10.18 pydantic/main.py:427", newSource: "pydantic@2.9.2 pydantic/main.py:352",
    links: [["Pydantic v2 migration · BaseModel changes", "https://docs.pydantic.dev/2.9/migration/#changes-to-pydanticbasemodel"]],
    note: "The old dict() method is retained as a deprecated alias in v2. Migrate the call site to model_dump().",
  },
};

export function sampleCitation(row: Dependency) {
  if (row.source !== "release") return null;
  if (row.name === "next" && row.ecosystem === "npm") return row.version === "15.0.0" ? upgrades.next.newSource : row.version === "14.2.3" ? upgrades.next.oldSource : null;
  if (row.name === "pydantic" && row.ecosystem === "PyPI") return row.version === "2.9.2" ? upgrades.pydantic.newSource : row.version === "1.10.18" ? upgrades.pydantic.oldSource : null;
  return null;
}

import { isMap, isScalar, parseDocument } from "yaml";

type AddPin = (name: unknown, version: unknown, ecosystem: "npm" | "Go", git?: boolean) => void;

const peerless = (version: string) => version.split("(", 1)[0].trim();

function splitSelector(selector: string) {
  const at = selector.indexOf("@", selector.startsWith("@") ? 1 : 0);
  return at < 0 ? undefined : [selector.slice(0, at), selector.slice(at + 1)] as const;
}

export function readPnpm(input: string, add: AddPin) {
  // Read only the maps this format uses (no YAML alias expansion). Preserve
  // the spelling of unquoted version scalars, including trailing zeroes.
  const doc = parseDocument(input, { schema: "failsafe" });
  if (doc.errors.length) throw new Error("Invalid pnpm lockfile");
  const root = doc.contents;
  if (!isMap(root)) return;
  const direct = new Map<string, string>();
  const importers = root.get("importers");
  for (const importer of isMap(importers) ? importers.items.map(pair => pair.value) : [root]) {
    if (!isMap(importer)) continue;
    for (const section of ["dependencies", "devDependencies", "optionalDependencies"]) {
      const dependencies = importer.get(section);
      if (!isMap(dependencies)) continue;
      for (const { key, value } of dependencies.items) {
        if (!isScalar(key) || typeof key.value !== "string" || direct.has(key.value)) continue;
        const pin = isMap(value) ? value.get("version") : isScalar(value) ? value.value : undefined;
        if (typeof pin !== "string") continue;
        const version = peerless(pin);
        if (version && !version.includes(":")) direct.set(key.value, version.split("/").at(-1) ?? "");
      }
    }
  }
  for (const [name, version] of direct) add(name, version, "npm");

  const packages = root.get("packages");
  const entries = isMap(packages) ? packages : root.get("snapshots");
  if (!isMap(entries)) return;
  for (const { key } of entries.items) {
    if (!isScalar(key) || typeof key.value !== "string") continue;
    const id = peerless(key.value.replace(/^\/+/, ""));
    const slash = id.lastIndexOf("/");
    const pin = splitSelector(id) ?? (slash < 0 ? undefined : [id.slice(0, slash), id.slice(slash + 1)]);
    if (pin) add(pin[0], pin[1], "npm");
  }
}

// Classic uses 'version "x"' (and may record Git only in 'resolved');
// Berry uses YAML-shaped 'version: x' and groups descriptors in one key.
export function readYarn(input: string, add: AddPin) {
  const isGit = (ref: string) => /git\+|git:\/\/|git@|github:|\.git(?:[#?]|$)/.test(ref);
  const unquote = (value: string) => value.trim().replace(/^"|"$/g, "");
  const isLocal = /^(?:workspace|file|link|portal):/;
  const seen = new Set<string>();
  let names = new Set<string>();
  let version = "";
  let git = false;
  function flush() {
    if (!version) return;
    for (const name of names) {
      const key = name + "@" + version;
      if (seen.has(key)) continue;
      seen.add(key);
      add(name, version, "npm", git);
    }
  }
  for (const line of input.split(/\r?\n/)) {
    if (!line.trim() || line.trimStart().startsWith("#")) continue;
    if (!/^\s/.test(line)) {
      flush();
      names = new Set();
      version = "";
      git = false;
      const header = line.trimEnd();
      if (!header.endsWith(":")) continue;
      const descriptors = header.slice(0, -1);
      for (const selector of descriptors.split(", ")) {
        const pin = splitSelector(unquote(selector));
        if (pin && !isLocal.test(pin[1])) {
          names.add(pin[0]);
          git ||= isGit(pin[1]);
        }
      }
    } else if (names.size) {
      const field = /^  (version|resolved|resolution):?\s+(.+)$/.exec(line);
      if (!field) continue;
      const ref = unquote(field[2]);
      if (field[1] === "version") version ||= ref;
      else {
        const locator = field[1] === "resolution" ? splitSelector(ref)?.[1] ?? ref : ref;
        if (isLocal.test(locator)) names.clear();
        else git ||= isGit(locator);
      }
    }
  }
  flush();
}

export function readGoMod(input: string, add: AddPin) {
  const requires: [string, string][] = [];
  const replacements = new Map<string, string>();
  let block: "require" | "replace" | undefined;
  for (const raw of input.split(/\r?\n/)) {
    const line = raw.split("//", 1)[0].trim();
    if (!line) continue;
    if (line === ")") { block = undefined; continue; }
    const directive = /^(require|replace)\s+(.+)$/.exec(line);
    const kind = block ?? directive?.[1];
    const spec = block ? line : directive?.[2];
    if ((kind !== "require" && kind !== "replace") || !spec) continue;
    if (spec === "(") { block = kind; continue; }
    if (kind === "require") {
      const [name, version, extra] = spec.split(/\s+/);
      if (name && version && !extra) requires.push([name, version]);
    } else {
      // Like the CLI, only show version overrides for the same module.
      const [lhs, rhs] = spec.split(/\s*=>\s*/);
      if (!rhs) continue;
      const [old] = lhs.split(/\s+/);
      const [name, version, extra] = rhs.split(/\s+/);
      if (old === name && version && !extra) replacements.set(name, version);
    }
  }
  for (const [name, version] of requires) add(name, replacements.get(name) ?? version, "Go");
}

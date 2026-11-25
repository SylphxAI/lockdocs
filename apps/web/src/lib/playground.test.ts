import assert from "node:assert/strict";
import { test } from "node:test";
import { detectLockfileFormat, formats, type Format } from "./lockfile-formats";
import { resolveLockfile, samples } from "./playground";
import { readPreviewFile } from "./playground-file";
import { filterPackages, matchingUpgrade } from "./playground-results";

function pins(format: Format, input: string) {
  return resolveLockfile(format, input).map(({ name, version, ecosystem, source }) =>
    [`${name}@${version}`, ecosystem, source]);
}

test("all eight Playground formats have working samples", () => {
  assert.equal(formats.length, 8);
  for (const format of formats) assert.ok(pins(format, samples[format]).length > 0, format);
});

test("pnpm v9 reads importer and package pins, unwrapping peers without duplicating rows", () => {
  assert.deepEqual(pins("pnpm-lock.yaml", `# pnpm workspace
lockfileVersion: '9.0'
importers:
  .:
    dependencies:
      '@example/ui': {specifier: ^2.0.0, version: 2.1.6(react@19.0.0)}
      local: {specifier: 'workspace:*', version: 'link:../local'}
  apps/shop:
    optionalDependencies:
      kleur: {specifier: ^4.0.0, version: 4.1.5}
    devDependencies:
      vite: {specifier: ^6.0.0, version: 6.0.7}
packages:
  '@example/ui@2.1.6': {resolution: {integrity: example}}
  react@19.0.0: {} # transitive
snapshots:
  '@example/ui@2.1.6(react@19.0.0)': {}
`), [
    ["@example/ui@2.1.6", "npm", "release"], ["kleur@4.1.5", "npm", "release"],
    ["react@19.0.0", "npm", "release"], ["vite@6.0.7", "npm", "release"],
  ]);
});

test("pnpm reads legacy slash keys, current at-keys, and the snapshots fallback", () => {
  assert.deepEqual(pins("pnpm-lock.yaml", `lockfileVersion: 5.4
dependencies:
  zod: /zod/3.21.4
packages:
  /zod/3.21.4: {}
  '/@scope/base/1.2.0': {}
  /react@18.3.1: {}
`), [["@scope/base@1.2.0", "npm", "release"], ["react@18.3.1", "npm", "release"], ["zod@3.21.4", "npm", "release"]]);
  assert.deepEqual(pins("pnpm-lock.yaml", "snapshots: {'foo@1.10(peer@2.0.0)': {}}"), [["foo@1.10", "npm", "release"]]);
  assert.deepEqual(pins("pnpm-lock.yaml", "dependencies: {foo: 'file:../foo', bar: ^2.0.0}"), []);
  assert.throws(() => pins("pnpm-lock.yaml", "packages: ["), /Invalid pnpm lockfile/);
});

test("Yarn Classic handles grouped scoped descriptors, comments, and Git resolutions", () => {
  assert.deepEqual(pins("yarn.lock", `# yarn lockfile v1

"@scope/core@^7.0.0", "@scope/core@^7.1.0":
  version "7.24.0"
  dependencies:
    version "9.9.9"

example@^2.0.0:
  version "2.4.1"
  resolved "https://github.com/sample/example.git#0123456789"

"my-app@link:../my-app":
  version "1.0.0"
`), [["@scope/core@7.24.0", "npm", "release"], ["example@2.4.1", "npm", "git"]]);
});

test("Yarn Berry skips metadata, source-only workspace entries, and broken stanzas", () => {
  assert.deepEqual(pins("yarn.lock", `__metadata:
  version: 8
"@scope/ui@npm:^2.0.0, @scope/ui@npm:^2.1.0":
  version: 2.2.0
  resolution: "@scope/ui@npm:2.2.0"
"workspace-root@workspace:.":
  version: 0.0.0-use.local
"broken@npm:^1.0.0":
  dependencies:
    version: 9.9.9
`), [["@scope/ui@2.2.0", "npm", "release"]]);
  assert.deepEqual(pins("yarn.lock", "not a stanza\n  version: 1.2.3"), []);
});

test("Go reads single and block requirements, comments, indirect pins and same-module overrides", () => {
  assert.deepEqual(pins("go.mod", `module example.com/service

go 1.22.0
require github.com/gin-gonic/gin v1.10.0 // comment
require (
  github.com/spf13/cobra v1.8.1
  golang.org/x/sys v0.20.0 // indirect
  example.com/tools/v2 v2.0.0-20241125100000-123456789abc
  example.com/legacy v2.0.1+incompatible
  no-version
  example.com/bad not-a-version
)
replace github.com/spf13/cobra v1.8.1 => github.com/spf13/cobra v1.8.0
replace (
  golang.org/x/sys => golang.org/x/sys v0.19.0 // overrides an indirect pin
  example.com/tools/v2 => ../local
  example.com/legacy => example.com/fork v2.0.2
)
`.replaceAll("\n", "\r\n")), [
    ["example.com/legacy@v2.0.1+incompatible", "Go", "release"],
    ["example.com/tools/v2@v2.0.0-20241125100000-123456789abc", "Go", "release"],
    ["github.com/gin-gonic/gin@v1.10.0", "Go", "release"],
    ["github.com/spf13/cobra@v1.8.0", "Go", "release"],
    ["golang.org/x/sys@v0.19.0", "Go", "release"],
  ]);
  assert.deepEqual(pins("go.mod", "require invalid v2\nrequire (\n  go.mod\n)"), []);
});

test("the input size limit also applies to the new formats", () => {
  for (const format of ["pnpm-lock.yaml", "yarn.lock", "go.mod"] as const) {
    assert.throws(() => pins(format, " ".repeat(400_001)), /400,000/);
  }
});

test("detects all eight canonical lockfile basenames case-insensitively", () => {
  for (const format of formats) {
    assert.equal(detectLockfileFormat(format), format);
    assert.equal(detectLockfileFormat("/project/" + format.toUpperCase()), format);
    assert.equal(detectLockfileFormat("project\\" + format.toUpperCase()), format);
  }
  for (const name of ["", "lock.json", "prod-requirements.txt", "yarn.lock.bak", "Cargo.lock/other"]) {
    assert.equal(detectLockfileFormat(name), undefined, name);
  }
});

test("file import detects canonical names and keeps the chosen format for custom names", async () => {
  assert.deepEqual(await readPreviewFile(new File(["Pydantic==1.10.18"], "REQUIREMENTS.TXT"), "go.mod"), {
    kind: "ready", format: "requirements.txt", text: "Pydantic==1.10.18",
  });
  assert.deepEqual(await readPreviewFile(new File(["pytest==8.3.3"], "prod-requirements.txt"), "requirements.txt"), {
    kind: "ready", format: "requirements.txt", text: "pytest==8.3.3",
  });
});

test("file import rejects oversize bytes before reading and never truncates decoded text", async () => {
  const oversizedFile = new File(["é".repeat(200_001)], "requirements.txt");
  let read = false;
  assert.deepEqual(await readPreviewFile({ name: oversizedFile.name, size: oversizedFile.size, text: async () => {
    read = true;
    return "";
  } }, "go.mod"), { kind: "oversize" });
  assert.equal(read, false);
  assert.deepEqual(await readPreviewFile({ name: "yarn.lock", size: 1, text: async () => "x".repeat(400_001) }, "go.mod"), { kind: "oversize" });
  const exact = "x".repeat(400_000);
  assert.deepEqual(await readPreviewFile(new File([exact], "yarn.lock"), "go.mod"), { kind: "ready", format: "yarn.lock", text: exact });
  assert.deepEqual(await readPreviewFile({ name: "yarn.lock", size: 1, text: async () => { throw new Error("read failed"); } }, "go.mod"), { kind: "unreadable" });
});

test("package filtering matches each displayed field, ignoring case and surrounding spaces", () => {
  const rows = [
    ...resolveLockfile("package-lock.json", samples["package-lock.json"]),
    ...resolveLockfile("requirements.txt", "Pydantic==1.10.18"),
  ];
  assert.equal(filterPackages(rows, "  "), rows);
  assert.deepEqual(filterPackages(rows, "  DANTIC ").map(row => row.name), ["pydantic"]);
  assert.deepEqual(filterPackages(rows, "10.18").map(row => row.name), ["pydantic"]);
  assert.deepEqual(filterPackages(rows, "PyPi").map(row => row.name), ["pydantic"]);
  assert.deepEqual(filterPackages(rows, "missing"), []);
});

test("contextual examples match exact ecosystem and package names with fixed precedence", () => {
  const unrelated = resolveLockfile("requirements.txt", "next==1.0.0\nzod==1.0.0\nreact==1.0.0");
  const wrongEcosystem = resolveLockfile("yarn.lock", 'pydantic@^1.0.0:\n  version "1.0.0"\nreact-native@^1.0.0:\n  version "1.0.0"');
  const react = resolveLockfile("pnpm-lock.yaml", samples["pnpm-lock.yaml"]);
  const zod = resolveLockfile("yarn.lock", samples["yarn.lock"]);
  const pydantic = resolveLockfile("requirements.txt", "Pydantic==1.10.18");
  const next = resolveLockfile("package-lock.json", samples["package-lock.json"]);
  assert.equal(matchingUpgrade([...unrelated, ...wrongEcosystem]), undefined);
  assert.equal(matchingUpgrade(react), "react");
  assert.equal(matchingUpgrade(zod), "zod");
  assert.equal(matchingUpgrade(pydantic), "pydantic");
  assert.equal(matchingUpgrade(next), "next");
  assert.equal(matchingUpgrade([...react, ...zod]), "zod");
  assert.equal(matchingUpgrade([...react, ...zod, ...pydantic]), "pydantic");
  assert.equal(matchingUpgrade([...react, ...zod, ...pydantic, ...next]), "next");
});

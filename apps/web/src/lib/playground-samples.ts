import type { Format } from "./lockfile-formats";
import { upgrades, type UpgradeId } from "./upgrade-examples";

const npmSample = (next: string) => JSON.stringify({ name: "my-next-app", lockfileVersion: 3, packages: {
    "": { dependencies: { next, react: "18.3.1", zod: "4.6.7" } },
    "node_modules/next": { version: next }, "node_modules/react": { version: "18.3.1" }, "node_modules/zod": { version: "4.6.7" },
  } }, null, 2);
const uvSample = (version: string) => "version = 1\n\n[[package]]\nname = \"pydantic\"\nversion = \"" + version + "\"\nsource = { registry = \"https://pypi.org/simple\" }\n\n[[package]]\nname = \"annotated-types\"\nversion = \"0.7.0\"\nsource = { registry = \"https://pypi.org/simple\" }";

export const samples: Record<Format, string> = {
  "package-lock.json": npmSample("15.0.0"),
  "pnpm-lock.yaml": `lockfileVersion: '9.0'

importers:
  .:
    dependencies:
      react-dom:
        specifier: ^18.3.0
        version: 18.3.1(react@18.3.1)

packages:
  react-dom@18.3.1:
    peerDependencies:
      react: ^18.3.1
  react@18.3.1: {}
`,
  "yarn.lock": `# yarn lockfile v1

"@babel/core@^7.24.0", "@babel/core@^7.24.1":
  version "7.24.1"
  resolved "https://registry.yarnpkg.com/@babel/core/-/core-7.24.1.tgz"

zod@^3.23.0:
  version "3.23.8"
  resolved "https://registry.yarnpkg.com/zod/-/zod-3.23.8.tgz"
`,
  "go.mod": `module example.com/my-service

go 1.22.0

require (
    github.com/spf13/cobra v1.8.1
    golang.org/x/sys v0.20.0 // indirect
)

replace github.com/spf13/cobra => github.com/spf13/cobra v1.8.0
`,
  "bun.lock": "{\n  \"lockfileVersion\": 1,\n  \"workspaces\": {\"\": {\"dependencies\": {\"next\": \"14.2.3\"}}},\n  \"packages\": {\n    \"next\": [\"next@14.2.3\", \"\", {}],\n    \"zod\": [\"zod@4.6.7\", \"\", {}],\n  },\n}",
  "Cargo.lock": "version = 4\n\n[[package]]\nname = \"tokio\"\nversion = \"1.44.2\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.219\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"",
  "uv.lock": uvSample("2.9.2"),
  "requirements.txt": "pydantic==1.10.18\nrequests==2.32.3\n",
};

const upgradeFormats = {
  next: "package-lock.json", pydantic: "uv.lock", zod: "yarn.lock", react: "pnpm-lock.yaml",
} satisfies Record<UpgradeId, Format>;

export function upgradeSample(id: UpgradeId) {
  const format = upgradeFormats[id];
  // The lockfile pins the release cited by the “before” half of the report.
  const pin = upgrades[id].oldSource.split(" ")[0];
  const version = pin.slice(pin.lastIndexOf("@") + 1);
  const input = id === "next" ? npmSample(version) : id === "pydantic" ? uvSample(version) : samples[format];
  const entry = { next: '"node_modules/next"', pydantic: "[[package]]", zod: "zod@", react: pin + ":" }[id];
  const lines = input.split("\n");
  const start = lines.findIndex(line => line.trimStart().startsWith(entry));
  return { format, input, pin, snippet: lines.slice(start, start + 3).join("\n") };
}

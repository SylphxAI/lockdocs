import assert from "node:assert/strict";
import { test } from "node:test";
import { privateProofExamples } from "../src/lib/private-proof-examples";

const cases = [
  ["npm", "npm", "@acme/session", "session renewal", [".npmrc", "NPM_CONFIG_*"]],
  ["pypi", "PyPI", "acme-auth", "token refresh", ["uv.toml", "pip.conf", "PIP_INDEX_URL", "~/.netrc"]],
  ["cargo", "Cargo", "acme-rpc", "retry policy", [".cargo/config.toml", "sparse", "~/.cargo/credentials.toml"]],
  ["go", "Go", "corp.example/sdk", "client config", ["GOPROXY", "GOPRIVATE", "~/.netrc"]],
] as const;

test("private pricing proof covers exactly the four registry examples in order", () => {
  assert.deepEqual(Object.keys(privateProofExamples), cases.map(([id]) => id));
});

for (const [id, label, pkg, query, settings] of cases) {
  test(label + " shows exact offline commands and bilingual registry setup", () => {
    const example = privateProofExamples[id];
    assert.equal(example.label, label);
    assert.equal(example.fetch, "lockdocs fetch " + pkg);
    assert.equal(example.search, `lockdocs docs "${query}" --pkg ${pkg} --offline`);
    const [en, zh] = example.setup;
    assert.match(en, /^Uses /);
    assert.match(zh, /[一-鿿]/u);
    for (const setting of settings) {
      assert.ok(en.includes(setting), "missing English setting: " + setting);
      assert.ok(zh.includes(setting), "missing Traditional Chinese setting: " + setting);
    }
  });
}

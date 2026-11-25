// Bundled examples for the private sources documented in docs/pro.md.
export const privateProofExamples = {
  npm: {
    label: "npm",
    fetch: "lockdocs fetch @acme/session",
    search: 'lockdocs docs "session renewal" --pkg @acme/session --offline',
    setup: [
      "Uses your existing .npmrc and NPM_CONFIG_* settings.",
      "沿用現有的 .npmrc 和 NPM_CONFIG_* 設定。",
    ],
  },
  pypi: {
    label: "PyPI",
    fetch: "lockdocs fetch acme-auth",
    search: 'lockdocs docs "token refresh" --pkg acme-auth --offline',
    setup: [
      "Uses your existing uv.toml, pip.conf, PIP_INDEX_URL and ~/.netrc settings.",
      "沿用現有的 uv.toml、pip.conf、PIP_INDEX_URL 和 ~/.netrc 設定。",
    ],
  },
  cargo: {
    label: "Cargo",
    fetch: "lockdocs fetch acme-rpc",
    search: 'lockdocs docs "retry policy" --pkg acme-rpc --offline',
    setup: [
      "Uses sparse registries from .cargo/config.toml and credentials from ~/.cargo/credentials.toml.",
      "沿用 .cargo/config.toml 設定的 sparse 套件庫，以及 ~/.cargo/credentials.toml 裡的憑證。",
    ],
  },
  go: {
    label: "Go",
    fetch: "lockdocs fetch corp.example/sdk",
    search: 'lockdocs docs "client config" --pkg corp.example/sdk --offline',
    setup: [
      "Uses your existing GOPROXY, GOPRIVATE and ~/.netrc settings.",
      "沿用現有的 GOPROXY、GOPRIVATE 和 ~/.netrc 設定。",
    ],
  },
} as const;

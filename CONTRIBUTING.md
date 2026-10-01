# Contributing to lockdocs

lockdocs answers from the exact library versions a project pins, locally.
Start with the [README](README.md), [vision](docs/vision.md) and
[AGENTS.md](AGENTS.md); [PROJECT.md](PROJECT.md) explains the layout.

## Find the right place

- Search [existing issues](https://github.com/SylphxAI/lockdocs/issues) and pull
  requests before starting. The [good first issue list](https://github.com/SylphxAI/lockdocs/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22good%20first%20issue%22)
  contains tasks when suitable ones are available.
- Use [Discussions](https://github.com/SylphxAI/lockdocs/discussions) for usage
  questions and early ideas. For a concrete bug or feature, use the
  [issue forms](https://github.com/SylphxAI/lockdocs/issues/new/choose).
- Include the version, ecosystem, lockfile format, package@version, command/tool
  arguments and expected citations. A minimal public or synthetic project is
  best; say whether the package is installed, fetching is enabled and the
  installed version matches the lockfile version. Redact private package names, registry
  URLs, personal paths, tokens and private source from examples and logs.
- Report vulnerabilities through the [security policy](https://github.com/SylphxAI/lockdocs/security/policy),
  not a public issue.

## Make a focused change

Fork the repository and create a branch from `main`. Small fixes can go straight
to a pull request; discuss new lockfile formats, public tool changes and larger
work first. Add a regression test for changed behavior and update the relevant
docs. Keep the three-tool surface and versioned cache rules in
[AGENTS.md](AGENTS.md).

Rust stable builds the engine and CLI. Bun 1.4.0 runs the docs and repository
scripts. From your checkout, the relevant checks are:

```bash
cargo test -p lockdocs-core
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --locked -- -D warnings
bun install --frozen-lockfile
bun scripts/check-version.ts
bun scripts/check-capabilities.ts
bun run docs:build
```

Use small synthetic fixtures for parser/extraction tests. A new lockfile format
needs a pure parser, a unit test and registration in `LOCKFILES` or `parse`.
Ranking changes are judged on the whole benchmark in CI, not a single question;
held-out questions are not tuning data.

## Open the pull request

Target `main`, link the issue if there is one, and describe the version-correct
result. List the checks you ran and any you could not run. CI covers the Rust
suite on supported platforms, formatting/lints, CLI/MCP/launcher smoke tests,
manifest checks and docs; merge follows the repository's normal queue.
Contributions use the repository's [MIT license](LICENSE) and the organization
[code of conduct](https://github.com/SylphxAI/.github/blob/main/.github/CODE_OF_CONDUCT.md).

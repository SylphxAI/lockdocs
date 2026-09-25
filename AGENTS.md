# lockdocs: agent notes

Read [PROJECT.md](PROJECT.md) for the layout and the release flow.

- Run the narrowest check first: `cargo test -p lockdocs-core`, then
  `cargo test --workspace`. The benchmark (`bench/`) installs real packages;
  run it in CI (`bench.yml`), not on a shared machine.
- Keep the MCP surface at three tools: `resolve`, `docs`, `api`.
- Bump `index::FORMAT` whenever extraction output or `Entry` changes, so old
  caches are rebuilt; bump `upstream::FORMAT` when `fetch` downloads more, so
  `lockdocs fetch` refreshes old copies.
- Ranking changes are judged on the whole benchmark in CI, never on one
  question; questions marked `held-out` are not used for tuning.
- A new lockfile format: add a pure parser to `lockfile.rs` with a unit test,
  and register it in `LOCKFILES` or `parse`.
- One version everywhere: `bun scripts/set-version.ts` (CI runs
  `scripts/check-version.ts`).
- Format with `rustfmt --edition 2021 <files>` (config in `rustfmt.toml`).
- Never commit secrets, tokens or `.env` files.

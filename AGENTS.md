# lockdocs: agent notes

lockdocs answers an agent's library questions from the exact versions in the
project's lockfile, locally and offline, over a three-tool MCP surface. Work
here is judged by the version-sensitive benchmark: the answer must be right for
the pinned version and cheap in tokens and time. Layout and release flow:
[PROJECT.md](PROJECT.md); destination: [docs/vision.md](docs/vision.md).

## Hard lines

- The MCP surface stays at three tools (`resolve`, `docs`, `api`): agents pay
  for every tool description on every call, so new abilities go into these.
- Bump `index::FORMAT` when extraction output or `Entry` changes, and
  `upstream::FORMAT` when `fetch` downloads more: otherwise old caches serve
  stale answers.
- Questions marked `held-out` in `bench/questions.json` are not used for tuning:
  they are the check that ranking changes generalize.
- No secrets, tokens or `.env` files in the repository.

## How a result is judged

- `cargo test -p lockdocs-core`, then `cargo test --workspace`; format with
  `rustfmt --edition 2021` (see `rustfmt.toml`).
- Ranking changes are judged on the whole benchmark (`bench.yml`, which installs
  real packages, so run it in CI), never on one question.
- CI also runs `scripts/check-version.ts`, `scripts/check-capabilities.ts`,
  `scripts/check-tagline.ts` and `scripts/check-readme-links.ts` (README URLs must be absolute for npm). One version everywhere: `bun scripts/set-version.ts`.
- A new lockfile format is a pure parser in `lockfile.rs` with a unit test,
  registered in `LOCKFILES` or `parse`.

# lockdocs vision

## What we are building

lockdocs gives AI coding agents the documentation for the exact library
versions a project uses. It reads the project's lockfile, finds each package on
disk (or downloads that exact version when asked), and answers questions from
that version's own files: its README, docs folders, type declarations and
doc comments, plus the upstream docs at the version's git tag.

An agent that writes code for Next.js 14 should get the Next.js 14 API, not the
newest one. That is the one job.

## Who it is for

- Developers who use an AI coding agent (Claude Code, Codex, Cursor, VS Code,
  Windsurf, Gemini CLI) on projects that do not track the newest release of
  every dependency.
- Teams that cannot send their dependency list to a hosted service, or that
  hit rate limits on one.

## Boundaries

- Local first: it runs on the developer's machine, needs no account or API
  key, and works offline after the one-time downloads.
- Network use is opt-in and limited to public sources: package registries,
  GitHub (docs folders at a tag, and official docs-site repositories), and the
  embedding model on Hugging Face.
- Three MCP tools only: `resolve`, `docs`, `api`. New abilities go into those
  tools, not into more tools.
- Four ecosystems: npm, PyPI, Cargo and Go. A new ecosystem needs a lockfile
  parser, a way to find installed files, and a symbol extractor.
- No hosted index. We do not crawl or store other people's documentation on a
  server.

## What good looks like

- On the version-sensitive benchmark (`bench/`), lockdocs is correct at least
  as often as the best hosted alternative on every column: older majors, newer
  majors, single-version libraries and the held-out questions. Results are
  measured on GitHub-hosted runners and published as they come out.
- A question costs tens of milliseconds and about a thousand tokens.
- Every answer cites `package@version path:line`, so an agent or a person can
  check it.

Current capabilities and their code: [capabilities.md](capabilities.md).

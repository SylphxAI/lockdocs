# Security

## Reporting

Report vulnerabilities privately via GitHub Security Advisories on
[SylphxAI/lockdocs](https://github.com/SylphxAI/lockdocs/security/advisories/new).
Do not open public issues for sensitive reports.

## Boundary

- lockdocs reads lockfiles and installed package files. It makes no network
  calls unless fetching is enabled (`--fetch`, `LOCKDOCS_FETCH=1`).
- With fetching enabled it downloads only the exact package versions requested,
  from registry.npmjs.org, pypi.org / files.pythonhosted.org, static.crates.io
  and proxy.golang.org, over HTTPS. Archives are unpacked with path checks (no
  absolute paths, no `..`), only documentation and source files are written,
  and downloads are capped at 64 MB.
- Subprocesses: the system `python3` once (to list its site-packages; disable
  with `LOCKDOCS_NO_SYSTEM_PYTHON=1`), and the `claude` CLI in `lockdocs setup`.
- The MCP server speaks stdio only.
- The cache lives in your OS cache directory (`LOCKDOCS_CACHE` overrides it)
  and holds extracted docs and symbols of your dependencies.

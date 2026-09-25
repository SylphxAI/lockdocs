# Fetching (opt-in)

By default lockdocs makes no network calls. When a pinned package is not on disk, the answer says what to install:

```text
missing-pkg@2.0.0 is pinned in package-lock.json but its files are not on this machine:
run your package manager's install, or enable fetching (--fetch or LOCKDOCS_FETCH=1)
to download exactly this version.
```

With fetching enabled (`--fetch`, `LOCKDOCS_FETCH=1`, or `lockdocs setup --fetch` for the MCP server), lockdocs downloads exactly the pinned version once and caches it:

| Ecosystem | Source |
|---|---|
| npm | the version's tarball from `registry.npmjs.org` |
| PyPI | a pure-Python wheel, else any wheel, else the sdist, from `pypi.org` |
| crates.io | the `.crate` from `static.crates.io` |
| Go | the module zip from `proxy.golang.org` |

Only docs and source files are unpacked (no binaries, nothing outside the target directory, 64 MB download cap). Answers from fetched packages are labelled `fetched from <registry>`.

Fetching also lets you ask about a version you do not use, which is handy when planning an upgrade:

```bash
lockdocs npm:zod@4.1.5 "reject unknown keys" --fetch
lockdocs pypi:pydantic@2.9.2 "model config extra fields" --fetch
```

A fetched copy is used offline afterwards, even without `--fetch`.

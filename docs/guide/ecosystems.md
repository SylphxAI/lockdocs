# Ecosystems

## npm

- **Versions:** `package-lock.json` (v1-v3), `npm-shrinkwrap.json`, `pnpm-lock.yaml` (v5-v9, including peer suffixes), `bun.lock`, `yarn.lock` (v1 and Berry). With no lockfile, `package.json` plus what is in `node_modules`. Workspace roots above the current directory count.
- **Sources:** `node_modules/<name>` here or in a parent directory, and pnpm's `node_modules/.pnpm/<name>@<version>` store, so the exact version is found even when another is hoisted. Yarn Plug'n'Play projects work without `node_modules`: the package's zip in `.yarn/cache` (or the global Berry cache, `$YARN_CACHE_FOLDER`) is read once and its docs and declarations are unpacked into the lockdocs cache.
- **Docs:** README, CHANGELOG, HISTORY, MIGRATION and UPGRADING files, `docs/` folders, other root-level Markdown.
- **API:** `.d.ts`, `.d.mts` and `.d.cts` (deduplicated) with JSDoc: functions, classes and members, interfaces and members, type aliases, enums, constants, namespaces and `export { a as b }` re-exports, which `api` follows to the target. When a package ships no declarations, `@types/<name>` is used; failing that, JSDoc-documented JavaScript.

## PyPI

- **Versions:** `uv.lock`, `poetry.lock`, `pdm.lock`, `Pipfile.lock`, pinned `requirements*.txt`. With none, the packages installed in the project's virtualenv (direct dependencies from `pyproject.toml`).
- **Sources:** `.venv`, `venv`, `env` in the project or its parents, `$VIRTUAL_ENV`, `$CONDA_PREFIX`, then the system interpreter's site-packages. Files come from the wheel's `RECORD`, so only that distribution's files are read.
- **Docs:** the README embedded in the wheel's `METADATA`, module docstrings.
- **API:** classes, functions and methods with their full signatures (decorators, defaults, annotations) and docstrings, from `.py` and `.pyi` (stubs when there is no source). Private names are skipped, `__init__` and `__call__` are kept.

## crates.io

- **Versions:** `Cargo.lock`. Direct dependencies are the ones your workspace members list.
- **Sources:** `$CARGO_HOME/registry/src/*/<name>-<version>` (what `cargo fetch` or any build downloads), or `vendor/`. Git dependencies (`source = "git+..."` in `Cargo.lock`) are found in `$CARGO_HOME/git/checkouts`, matched by crate name and version, including crates inside a checked-out workspace.
- **Docs:** README and CHANGELOG, crate and module docs (`//!`), which for many crates are the real guide.
- **API:** public functions, methods of inherent impls, structs, enums, traits and their methods, type aliases, constants, public modules and exported `macro_rules!`, with `///` docs. Items inside configuration macros (`cfg_rt! { ... }`) are parsed too. `#[doc(hidden)]` is respected.

## Go

- **Versions:** `go.mod` (`require` blocks, `// indirect`, same-module `replace` versions).
- **Sources:** `$GOMODCACHE`, `$GOPATH/pkg/mod` or `~/go/pkg/mod` (with Go's case escaping), or `vendor/`.
- **Docs:** README, package doc comments.
- **API:** exported functions, methods (`Context.JSON`), types, interface methods, constants and variables with doc comments. `internal/`, `testdata/` and `_test.go` are skipped.

## Legacy copies

Some packages bundle the previous major for migration: `zod/v3` inside zod 4, `pydantic/v1` inside pydantic 2. Paths with a `vN` segment below the package's major version rank below the current API, so answers default to the version you use.

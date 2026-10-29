# Version-sensitive benchmark checks

Run the whole benchmark when changing API selection, extraction, or ranking.
The GitHub benchmark workflow installs the pinned projects and exercises the
package-only, first-use, and fetched-docs paths. Keep its existing graders and
floors; do not use held-out questions to tune a resolver change.

Public import applicability also has a separate regression check:

```sh
cargo test -p lockdocs-core --test public_api
cargo build -p lockdocs
python3 scripts/check-public-api.py target/debug/lockdocs
```

The Python check installs isolated packages, then compares offline CLI and MCP
answers to the actual declaration files and Node runtime exports. It does not
change benchmark questions, fixture pins, scores, or public results. To reuse
already-installed projects without network setup, pass their directories after
the binary path.

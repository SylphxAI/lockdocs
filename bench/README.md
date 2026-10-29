# Benchmark fixture integrity

`projects/` contains frozen measurement inputs, not applications to keep on the
latest release. `fixture-lines.json` is the complete census of the 29 projects
and every direct dependency's intended line, including companion packages.
The lines follow the version-specific questions in `questions.json`: notably
Axum 0.7/0.8, FastAPI 0.88/0.115, Django 4.2/5.1 and SQLAlchemy 1.4/2.0 must not
collapse into one line. Pydantic 1/2 and the npm major pairs must stay distinct.

Run the existing network-free CI tests before installing or measuring:

```sh
python3 bench/test_run.py
python3 bench/test_cloud.py
# Or just the manifest check (Python 3.11+):
python3 bench/fixtures.py
```

The check requires exact stable pins, complete project and question-package
coverage, the intended version lines, and identical Prisma CLI/client,
React/React DOM and React Router/DOM companion versions. Type packages have
explicit lines too. Tests mutate every direct dependency across its line and
check that the failure names the project and package. `setup.sh` also checks
before any install or network call. No installs or benchmark scores are needed
for this guard.

The restored Python and Cargo pins are from the original fixture manifests and
the recorded versions in `docs/public/bench-results.json`; the Express 4 types
and Prisma 5/6 clients are restored to their original companion pins. Other
compatible same-line npm pins are left alone. This repair does **not** rerun the
benchmark, revise the questions or held-out set, or update published scores.
The recorded scores describe the recorded run, not a new measurement.

## Dependency bots

`.github/renovate.json` excludes only the benchmark inputs and synthetic Rust
integration-test fixtures (plus Renovate's normal installed-dependency ignores).
`.github/dependabot.yml` ignores all dependencies in those fixture directories,
including security updates, with no version-update PR slots. It does not
reintroduce Dependabot version updates elsewhere: Renovate remains responsible
for runtime, docs, toolchain and action updates. Existing bot PRs are not changed
by these files; the normal CI check rejects cross-line changes they propose.

To change a fixture deliberately, keep exact pins and compatible companions,
check package-registry resolution, and review the version-line census alongside
the relevant questions. A same-line patch pin can change without changing the
census. A new measurement must actually run the benchmark; a passing integrity
check is not an accuracy result. Never install the synthetic test fixtures from
a package registry.

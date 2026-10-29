#!/usr/bin/env python3
"""Network-free tests of benchmark isolation and regression gates."""
import contextlib
import fnmatch
import importlib.util
import io
import json
import os
import re
import shutil
import subprocess
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import fixtures

spec = importlib.util.spec_from_file_location("bench_runner", Path(__file__).with_name("run.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class BenchmarkTests(unittest.TestCase):
    def test_floors_apply_only_to_full_suite(self):
        runner.check_floors({"fetched": {"passed": 96}, "hybrid": {"passed": 62}}, 105)
        runner.check_floors({"fetched": {"passed": 0}}, 1)
        for variant, score in [("fetched", 95), ("hybrid", 61)]:
            with self.assertRaises(RuntimeError):
                runner.check_floors({variant: {"passed": score}}, 105)

    def test_hybrid_never_scores_below_keyword(self):
        ok = {"keyword": {"passed": 62}, "hybrid": {"passed": 62}, "fetched-keyword": {"passed": 96}, "fetched": {"passed": 97}}
        runner.check_floors(ok, 105)
        runner.check_floors({"keyword": {"passed": 70}, "hybrid": {"passed": 69}}, 1)  # partial runs are not gated
        for bad in [
            {"keyword": {"passed": 63}, "hybrid": {"passed": 62}},
            {"fetched-keyword": {"passed": 98}, "fetched": {"passed": 97}},
        ]:
            with self.assertRaises(RuntimeError):
                runner.check_floors(bad, 105)

    def test_fetch_formatter_preserves_flat_and_nested_file_counts(self):
        for package in [
            {"package": "axum@0.7.9", "files": 20},
            {"package": "axum@0.7.9", "upstream": {"files": 20}},
            {"package": "axum@0.7.9", "files": 20, "upstream": {"files": 20}},
        ]:
            self.assertEqual(runner.fetch_file_count(package), 20)
            result = {"tokenizer": "test", "runner": {"os": "Test", "machine": "test"},
                      "rows": [], "summary": {}, "variants": [], "index": {},
                      "fetch": {"axum07": {"ms": 1, "report": {"packages": [package]}}}}
            formatted = runner.markdown(result, False)
            self.assertIn("axum@0.7.9 20 files", formatted)
            self.assertNotIn("no upstream docs", formatted)
        self.assertEqual(runner.fetch_file_count({"files": 0, "upstream": {"files": 20}}), 0)

    def test_first_use_has_own_empty_cache_and_no_opt_in(self):
        with tempfile.TemporaryDirectory(prefix="lockdocs-bench-test-") as tmp:
            root = Path(tmp)
            (root / "questions.json").write_text(json.dumps({"questions": [{
                "id": "fixture", "project": "project", "package": "fixture",
                "question": "documented API", "why": "harness test", "expect": [["right_api"]],
            }]}))
            (root / "projects" / "project").mkdir(parents=True)
            default_cache = root / "regular-cache"
            default_cache.mkdir()
            (default_cache / "prefetched").write_text("already present")
            log = root / "calls.jsonl"
            binary = root / "lockdocs"
            binary.write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
cache = pathlib.Path(os.environ["LOCKDOCS_CACHE"])
cmd = sys.argv[1]
with open(os.environ["TEST_LOG"], "a") as log:
    log.write(json.dumps({"cmd": cmd, "cache": str(cache), "empty": not any(cache.iterdir()),
        "fetch": os.environ.get("LOCKDOCS_FETCH"), "token": os.environ.get("GITHUB_TOKEN"),
        "no_upstream": os.environ.get("LOCKDOCS_NO_UPSTREAM")}) + "\\n")
if cmd in ("index", "fetch"):
    print("{}")
else:
    (cache / "queried").write_text("cached")
    print("fixture@1.0.0 · npm · source\\nright_api")
''')
            binary.chmod(0o755)
            out = root / "results.json"
            with patch.object(runner, "HERE", str(root)), patch.object(sys, "argv", [
                "run.py", str(binary), str(root / "projects"), str(out), "--fetch",
            ]), patch.dict(os.environ, {
                "LOCKDOCS_CACHE": str(default_cache), "TEST_LOG": str(log),
                "LOCKDOCS_FETCH": "1", "GITHUB_TOKEN": "test-placeholder",
            }), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                runner.main()
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            cold = next(c for c in calls if c["cmd"] == "docs" and c["cache"] != str(default_cache))
            self.assertTrue(cold["empty"])
            self.assertIsNone(cold["fetch"])
            self.assertIsNone(cold["token"])
            self.assertIsNone(cold["no_upstream"])
            self.assertLess(calls.index(cold), next(i for i, c in enumerate(calls) if c["cmd"] == "fetch"))
            self.assertEqual(json.loads(out.read_text())["summary"]["first-use-default"]["passed"], 1)
            self.assertTrue((default_cache / "prefetched").exists())


class FixtureIntegrityTests(unittest.TestCase):
    @contextlib.contextmanager
    def fixture_copy(self):
        # Copy only metadata under test, never installs, caches, or result files.
        with tempfile.TemporaryDirectory(prefix="lockdocs-fixture-test-") as tmp:
            root = Path(tmp)
            for name in ("fixture-lines.json", "questions.json"):
                shutil.copyfile(fixtures.HERE / name, root / name)
            for project in (fixtures.HERE / "projects").iterdir():
                dest = root / "projects" / project.name
                dest.mkdir(parents=True)
                for name in fixtures.MANIFESTS:
                    if (project / name).is_file():
                        shutil.copyfile(project / name, dest / name)
            yield root

    def set_pin(self, root, project, package, version):
        directory = root / "projects" / project
        if (directory / "package.json").exists():
            path = directory / "package.json"
            data = json.loads(path.read_text())
            data["dependencies"][package] = version
            path.write_text(json.dumps(data))
        elif (directory / "requirements.txt").exists():
            path = directory / "requirements.txt"
            path.write_text(re.sub(r"(?m)^" + re.escape(package) + r"==.+$",
                                   f"{package}=={version}", path.read_text()))
        else:
            path = directory / "Cargo.toml"
            old = fixtures.dependencies(path)[package]
            path.write_text(re.sub(r"(?m)^" + re.escape(package) + r" = .+$",
                                   lambda m: m[0].replace(f'"={old}"', f'"={version}"'),
                                   path.read_text()))

    def test_renovate_excludes_fixtures_but_not_maintained_dependencies(self):
        repo = fixtures.HERE.parent
        config = json.loads((repo / ".github" / "renovate.json").read_text())
        patterns = config["ignorePaths"]
        def ignored(path):
            return any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns)
        for directory in (repo / "bench" / "projects", repo / "crates" / "lockdocs-core" / "tests"):
            for path in directory.rglob("*"):
                if path.name in (*fixtures.MANIFESTS, "go.mod", "Cargo.lock", "yarn.lock"):
                    self.assertTrue(ignored(path.relative_to(repo).as_posix()), path)
        for path in ("package.json", "Cargo.toml", "crates/lockdocs-core/Cargo.toml",
                     "crates/lockdocs/Cargo.toml", "docs/package.json", ".github/workflows/ci.yml"):
            self.assertFalse(ignored(path), path)

    def test_repository_fixture_integrity(self):
        # This is part of the existing CI command, not just tests of the checker.
        self.assertEqual(fixtures.check(), [])

    def test_every_direct_dependency_rejects_a_different_line(self):
        lines = json.loads((fixtures.HERE / "fixture-lines.json").read_text())
        for project, packages in lines.items():
            for package, line in packages.items():
                with self.subTest(project=project, package=package), self.fixture_copy() as root:
                    parts = [int(p) for p in line.split(".")]
                    parts[-1] += 1
                    wrong = ".".join(map(str, parts + [0] * (3 - len(parts))))
                    self.set_pin(root, project, package, wrong)
                    errors = fixtures.check(root)
                    self.assertTrue(any(f"{project}/{package}: expected line {line}, got {wrong}" in e
                                        for e in errors), errors)

    def test_matching_companion_updates_within_supported_lines_pass(self):
        with self.fixture_copy() as root:
            for pkg in ("prisma", "@prisma/client"):
                self.set_pin(root, "prisma6", pkg, "6.19.4")
            self.set_pin(root, "axum07", "axum", "0.7.8")
            self.set_pin(root, "django42", "django", "4.2.29")
            self.assertEqual(fixtures.check(root), [])

    def test_companion_patch_mismatch_fails(self):
        for project, package in [("react19", "react-dom"), ("prisma6", "@prisma/client"),
                                 ("rr6", "react-router-dom")]:
            with self.subTest(project=project), self.fixture_copy() as root:
                old = fixtures.dependencies(root / "projects" / project / "package.json")[package]
                major, minor, patch = old.split(".")
                self.set_pin(root, project, package, f"{major}.{minor}.{int(patch) + 1}")
                self.assertTrue(any("companion pins must match" in e for e in fixtures.check(root)))

    def test_unpinned_manifests_fail(self):
        for project, name, package in [
            ("zod3", "package.json", "zod"),
            ("axum07", "Cargo.toml", "axum"),
            ("pydantic1", "requirements.txt", "pydantic"),
        ]:
            with self.subTest(project=project), self.fixture_copy() as root:
                path = root / "projects" / project / name
                version = fixtures.dependencies(path)[package]
                old, new = {
                    "package.json": (f'"{version}"', f'"^{version}"'),
                    "Cargo.toml": (f'"={version}"', f'"{version}"'),
                    "requirements.txt": (f"=={version}", f">={version}"),
                }[name]
                self.assertIn(old, path.read_text())
                path.write_text(path.read_text().replace(old, new))
                errors = fixtures.check(root)
                self.assertTrue(any(project in e and "expected" in e for e in errors), errors)

    def test_census_cannot_silently_lose_or_gain_projects_and_packages(self):
        with self.fixture_copy() as root:
            shutil.rmtree(root / "projects" / "axum07")
            (root / "projects" / "unknown").mkdir()
            path = root / "projects" / "eslint8" / "package.json"
            data = json.loads(path.read_text())
            data["dependencies"] = {"unknown": "1.0.0"}
            path.write_text(json.dumps(data))
            errors = "\n".join(fixtures.check(root))
            for diagnostic in ("axum07", "unknown", "eslint8/eslint", "eslint8/unknown",
                               "has no direct dependency"):
                self.assertIn(diagnostic, errors)

    def test_cli_fails_with_project_and_package_diagnostics(self):
        with self.fixture_copy() as root:
            self.set_pin(root, "fastapi088", "fastapi", "0.143.3")
            result = subprocess.run([sys.executable, str(fixtures.HERE / "fixtures.py"),
                                     str(root / "projects")], text=True, capture_output=True)
            self.assertEqual(result.returncode, 1)
            self.assertIn("fastapi088/fastapi: expected line 0.88, got 0.143.3", result.stderr)


if __name__ == "__main__":
    unittest.main()

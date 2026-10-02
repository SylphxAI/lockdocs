#!/usr/bin/env python3
"""Network-free tests of benchmark isolation and regression gates."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

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


if __name__ == "__main__":
    unittest.main()

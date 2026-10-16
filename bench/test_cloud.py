#!/usr/bin/env python3
"""Network-free tests of the Sylphx Search cloud docs arm."""
import importlib.util
import json
import os
import unittest

from pathlib import Path

spec = importlib.util.spec_from_file_location("cloud_arm", Path(__file__).with_name("cloud.py"))
cloud = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cloud)


class GradingTests(unittest.TestCase):
    def test_grading_matches_the_corpus_grader(self):
        question = {
            "expect": [["strictObject"], ["z.object("]],
            "reject": [".strict()"],
        }
        passed, missing, rejected = cloud.grade("z.strictObject({...}) inside z.object({})", question)
        self.assertTrue(passed)
        self.assertEqual(missing, [])
        self.assertEqual(rejected, [])

    def test_a_missing_group_and_a_reject_string_both_fail(self):
        question = {"expect": [["strictObject"]], "reject": ["z.strictObject("]}
        passed, missing, rejected = cloud.grade("run z.strictObject() instead", question)
        self.assertFalse(passed)
        # a matched group is absent from the missing list; the reject string is recorded
        self.assertEqual(missing, [])
        self.assertEqual(rejected, ["z.strictObject("])
        passed, missing, rejected = cloud.grade("plain object", question)
        self.assertFalse(passed)
        self.assertEqual(missing, [["strictObject"]])

    def test_citations_carry_every_field_the_grader_reads(self):
        text = cloud.cite(
            [
                {"url": "https://example.test/1", "title": "Docs", "snippet": "strictObject"},
                {"url": "https://example.test/2", "title": None, "snippet": None},
            ]
        )
        self.assertIn("https://example.test/1", text)
        self.assertIn("strictObject", text)
        self.assertIn("2. https://example.test/2", text)

    def test_the_corpus_keeps_its_pinned_versions(self):
        with open(Path(__file__).with_name("questions.json")) as handle:
            corpus = json.load(handle)
        self.assertEqual(len(corpus["questions"]), 105)
        # Every project the corpus names has a question at the version
        # bench/setup.sh installs, which is what makes the twins comparable.
        versions = {}
        for question in corpus["questions"]:
            versions.setdefault(question["project"], set()).add(question["line"])
        for project, lines in versions.items():
            self.assertRegex(cloud.project_version(project), r"^\d", project)
            self.assertTrue(lines <= {"older", "newer", "single"}, project)


class VersionPinTests(unittest.TestCase):
    def test_every_project_resolves_a_version(self):
        with open(Path(__file__).with_name("questions.json")) as handle:
            corpus = json.load(handle)
        projects = sorted({q["project"] for q in corpus["questions"]})
        for project in projects:
            version = cloud.project_version(project)
            self.assertNotEqual(version, "unknown", f"{project} has no resolvable version")
            self.assertRegex(version, r"^\d")


if __name__ == "__main__":
    unittest.main()

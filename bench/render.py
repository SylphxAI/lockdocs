#!/usr/bin/env python3
"""Write a bench.json into docs/benchmarks.md between the results markers.

Usage: python3 bench/render.py bench.json [run-url]
"""
import json, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from run import markdown  # noqa: E402

res = json.load(open(sys.argv[1]))
url = sys.argv[2] if len(sys.argv) > 2 else ""
body = markdown(res, res.get("context7_calls") is not None)
body = body.replace("## lockdocs benchmark\n\n", "")
if url:
    body = f"From [this run]({url}).\n\n" + body
p = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "docs", "benchmarks.md")
doc = open(p).read()
doc = re.sub(r"<!-- results -->.*?(?=\n## Reading the results)", "<!-- results -->\n\n" + body + "\n", doc, flags=re.S)
open(p, "w").write(doc)
print(body.split("\n\n")[1] if "\n\n" in body else body)

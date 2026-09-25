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


# README: the headline table and counts, between <!-- bench:start --> and <!-- bench:end -->.
NAMES = {"next": "Next.js", "react-router": "React Router", "tailwindcss": "Tailwind CSS", "eslint": "ESLint", "sqlalchemy": "SQLAlchemy",
         "fastapi": "FastAPI", "django": "Django", "express": "Express", "vite": "Vite", "react": "React", "prisma": "Prisma", "@prisma/client": "Prisma"}
rows = res["rows"]
libs = []
for r in rows:
    n = NAMES.get(r["package"], r["package"])
    if n not in libs:
        libs.append(n)
single = sorted({NAMES.get(r["package"], r["package"]) for r in rows if r.get("line") == "single"})


def sub(key, line):
    rs = [r for r in rows if (r.get("set") == "held-out" if line == "held-out" else r.get("line") == line)]
    got = sum(1 for r in rs if (r.get("context7", {}) if key == "context7" else r["variants"].get(key, {})).get("pass"))
    return f"{got}/{len(rs)}"


def fmt_ms(v):
    return f"{v:,.0f} ms"


table = [
    f"{len(rows)} questions whose correct answer depends on the version, over {len(libs)} libraries ({', '.join(libs)}), each asked in a real project with that version installed. "
    "An answer passes when it contains the version-correct API and none of the other version's. Same questions and grader against Context7's anonymous API, "
    f"on a GitHub-hosted runner" + (f" ([run]({url}))" if url else "") + ":",
    "",
    f"| | correct | older majors | newer majors | {', '.join(single) or 'single version'} | held-out | median tokens | median latency |",
    "|---|---|---|---|---|---|---|---|",
]
for key, label in [("fetched", "lockdocs + `lockdocs fetch`"), ("hybrid", "lockdocs, package files only"), ("context7", "Context7 (anonymous)")]:
    a = res["summary"].get(key)
    if a:
        table.append(f"| {label} | {a['passed']}/{a['total']} | {sub(key, 'older')} | {sub(key, 'newer')} | {sub(key, 'single')} | {sub(key, 'held-out')} | {a['median_tokens']:,} | {fmt_ms(a['median_ms'])} |")
rp = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "README.md")
readme = open(rp).read()
readme = re.sub(r"<!-- bench:start -->.*?<!-- bench:end -->", "<!-- bench:start -->\n" + "\n".join(table) + "\n<!-- bench:end -->", readme, flags=re.S)
open(rp, "w").write(readme)

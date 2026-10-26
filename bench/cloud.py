#!/usr/bin/env python3
"""lockdocs benchmark: the Sylphx Search cloud docs arm.

The same corpus, the same grader and the same pinned versions as
`bench/run.py`, asked through the cloud index instead of the local one:
`POST {base}/v1/web/search` with `category: "code"` and the query form
`"<question> (<package>@<version>)"`, exactly as
`services/ai/evals/search/evals/docs/run.ts` in SylphxAI/cloud asks it.

Why it lives here: the cloud vertical's whole claim is *one index format and
one ranking, local and cloud*. This arm is how lockdocs itself checks that the
cloud side answers its own version-sensitive questions, and it feeds the
`search` rows of the cloud report.

Usage:
    SYLPHX_SEARCH_EVAL_KEY=... python3 bench/cloud.py \\
        --out cloud.json [--base-url https://api.sylphx.ai] [--depth standard] \\
        [--only id,id] [--concurrency 4]

Writes the same run shape as `bench/run.py` (`rows[].lockdocs` carries the
cloud arm's verdict) so `bench/render.py` and the cloud report read it the
same way, plus a `cloud` block with latency, tokens, source calls and cache
hits. The key is read from the environment, never printed, never written.
"""

import argparse
import json
import os
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_BASE = "https://api.sylphx.ai"
MAX_RESULTS = 10


def count(text: str) -> int:
    try:
        import tiktoken

        return len(tiktoken.get_encoding("o200k_base").encode(text))
    except Exception:
        return (len(text) + 3) // 4


def grade(text: str, question: dict):
    lowered = text.lower()
    missing = [g for g in question["expect"] if not any(a.lower() in lowered for a in g)]
    hit_reject = [r for r in question.get("reject", []) if r.lower() in lowered]
    return (not missing and not hit_reject), missing, hit_reject


def cite(results) -> str:
    return "\n".join(
        f"{i + 1}. {r.get('url', '')} {r.get('title') or ''} {r.get('snippet') or ''}"
        for i, r in enumerate(results)
    )


def ask(base: str, key: str, depth: str, timeout: float, question: dict, version: str) -> dict:
    body = json.dumps(
        {
            "query": f"{question['question']} ({question['package']}@{version})",
            "depth": depth,
            "category": "code",
            "max_results": MAX_RESULTS,
        }
    ).encode()
    request = urllib.request.Request(
        f"{base}/v1/web/search",
        data=body,
        headers={"authorization": f"Bearer {key}", "content-type": "application/json"},
        method="POST",
    )
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            payload = json.loads(response.read().decode("utf-8", "replace"))
            response_headers = response.headers
    except urllib.error.HTTPError as error:
        return {"error": f"HTTP {error.code}", "ms": round((time.perf_counter() - started) * 1000)}
    except Exception as error:  # noqa: BLE001 - recorded, not retried
        return {"error": str(error), "ms": round((time.perf_counter() - started) * 1000)}
    results = payload.get("results") or []
    text = cite(results)
    passed, missing, rejected = grade(text, question)
    usage = payload.get("usage") or {}
    return {
        "pass": passed,
        "missing": missing,
        "rejected": rejected,
        "tokens": count(text),
        "ms": round((time.perf_counter() - started) * 1000),
        "results": len(results),
        "source_calls": usage.get("source_calls"),
        "cached": payload.get("cached"),
        "version_header": response_headers.get("x-sylphx-release") or response_headers.get("x-release"),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default=os.environ.get("SYLPHX_SEARCH_EVAL_BASE_URL", DEFAULT_BASE))
    parser.add_argument("--out", required=True)
    parser.add_argument("--depth", default="standard", choices=["fast", "standard", "deep"])
    parser.add_argument("--only", help="comma-separated question ids")
    parser.add_argument("--limit", type=int, help="ask at most the first N questions (id order)")
    parser.add_argument("--concurrency", type=int, default=4)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()

    key = os.environ.get("SYLPHX_SEARCH_EVAL_KEY")
    if not key:
        print("set SYLPHX_SEARCH_EVAL_KEY to an Access key with scope ai:inference", flush=True)
        return 2

    questions = json.load(open(os.path.join(HERE, "questions.json")))["questions"]
    if args.only:
        wanted = set(args.only.split(","))
        questions = [q for q in questions if q["id"] in wanted]
    if args.limit:
        questions = questions[: args.limit]
    versions = {}
    for project in sorted({q["project"] for q in questions}):
        versions[project] = project_version(project)

    with ThreadPoolExecutor(max_workers=max(1, min(args.concurrency, 8))) as pool:
        answers = list(
            pool.map(
                lambda q: ask(
                    args.base_url, key, args.depth, args.timeout, q, versions.get(q["project"], "unknown")
                ),
                questions,
            )
        )

    rows = []
    for question, answer in zip(questions, answers):
        rows.append(
            {
                "id": question["id"],
                "project": question["project"],
                "package": question["package"],
                "version": versions.get(question["project"], "unknown"),
                "question": question["question"],
                "why": question["why"],
                "line": question.get("line", ""),
                "set": question.get("set", "tuning"),
                "grading": json.dumps([question["expect"], question.get("reject", [])]),
                "lockdocs": {k: answer[k] for k in ("pass", "missing", "rejected", "tokens", "ms") if k in answer},
                "cloud": answer,
            }
        )
        state = "PASS" if answer.get("pass") else answer.get("error") or "fail"
        print(f"{question['id']:<24} {state}", flush=True)

    with_latency = [r["cloud"]["ms"] for r in rows if "ms" in r["cloud"]]
    latencies = sorted(with_latency)
    result = {
        "schema": "lockdocs.bench.cloud/v1",
        "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "runner": {"os": os.uname().sysname, "machine": os.uname().machine},
        "base_url_host": urllib.parse.urlsplit(args.base_url).hostname,
        "depth": args.depth,
        "category": "code",
        "max_results": MAX_RESULTS,
        "summary": {
            "answered": len(rows),
            "passed": sum(1 for r in rows if r["cloud"].get("pass")),
            "total": len(rows),
            "median_ms": latencies[len(latencies) // 2] if latencies else None,
            "p95_ms": latencies[min(len(latencies) - 1, int(len(latencies) * 0.95))] if latencies else None,
            "mean_source_calls": (
                sum(r["cloud"]["source_calls"] for r in rows if r["cloud"].get("source_calls") is not None)
                / max(1, sum(1 for r in rows if r["cloud"].get("source_calls") is not None))
            ),
            "errors": sum(1 for r in rows if "error" in r["cloud"]),
        },
        "rows": rows,
    }
    with open(args.out, "w") as handle:
        json.dump(result, handle, indent=1)
    print(f"{args.out}: cloud {result['summary']['passed']}/{result['summary']['total']} pass")
    return 0


def project_version(project: str) -> str:
    """The version this benchmark project installs, read the way run.py reads the answer."""
    package_json = os.path.join(HERE, "projects", project, "package.json")
    if os.path.exists(package_json):
        data = json.load(open(package_json))
        deps = {**data.get("dependencies", {}), **data.get("devDependencies", {})}
        # The first dependency is the library under test in every npm project.
        for name in deps:
            if name in LIBRARY_NAMES:
                return deps[name].lstrip("^~=")
    requirements = os.path.join(HERE, "projects", project, "requirements.txt")
    if os.path.exists(requirements):
        for line in open(requirements):
            line = line.strip()
            if "==" in line:
                return line.split("==")[1]
    cargo = os.path.join(HERE, "projects", project, "Cargo.toml")
    if os.path.exists(cargo):
        for line in open(cargo):
            if line.strip().startswith("axum") or line.strip().startswith("tokio"):
                return line.split("=")[-1].strip().strip('"')
    return "unknown"


LIBRARY_NAMES = {
    "zod",
    "next",
    "react-router",
    "pydantic",
    "axum",
    "tokio",
    "tailwindcss",
    "eslint",
    "@prisma/client",
    "react",
    "vite",
    "express",
    "sqlalchemy",
    "django",
    "fastapi",
}

if __name__ == "__main__":
    import urllib.parse

    raise SystemExit(main())

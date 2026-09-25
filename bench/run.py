#!/usr/bin/env python3
"""lockdocs benchmark: version-specific questions against real installs.

Usage: python3 bench/run.py <lockdocs-binary> <projects-dir> <out.json> [--context7] [--only id,id]

Each question names a project (with dependencies installed at pinned versions)
and a package. An answer passes when it contains at least one string from
every `expect` group and none of the `reject` strings (case-insensitive).
Tokens are counted with tiktoken o200k_base when available, else chars/4.
Context7 is queried the way its MCP server does (library search, then
context for the question) on the anonymous tier; 429s are recorded, not retried.
"""
import json, os, subprocess, sys, time, urllib.parse, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))

try:
    import tiktoken
    _enc = tiktoken.get_encoding("o200k_base")
    def count(s): return len(_enc.encode(s))
    TOKENIZER = "tiktoken o200k_base"
except Exception:
    def count(s): return (len(s) + 3) // 4
    TOKENIZER = "chars/4"


def grade(text, q):
    t = text.lower()
    missing = [g for g in q["expect"] if not any(a.lower() in t for a in g)]
    hit_reject = [r for r in q.get("reject", []) if r.lower() in t]
    return (not missing and not hit_reject), missing, hit_reject


def installed_version(proj, pkg):
    p = os.path.join(proj, "node_modules", pkg, "package.json")
    if os.path.exists(p):
        return json.load(open(p))["version"]
    return None


def run_lockdocs(binary, proj, q):
    t = time.perf_counter()
    r = subprocess.run([binary, "docs", q["question"], "--pkg", q["package"], "-C", proj], capture_output=True, text=True)
    ms = (time.perf_counter() - t) * 1000
    return (r.stdout if r.returncode == 0 else r.stderr), ms, r.returncode


C7 = "https://context7.com/api/v2"
_c7_ids = {}
c7_state = {"calls": 0, "rate_limited": 0, "errors": 0, "stopped": False, "limit": None, "remaining": None}


def c7_get(url):
    if c7_state["stopped"]:
        return None, 0, "skipped after rate limit"
    req = urllib.request.Request(url, headers={"User-Agent": "lockdocs-bench (+https://github.com/SylphxAI/lockdocs)"})
    t = time.perf_counter()
    c7_state["calls"] += 1
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            body = r.read().decode("utf-8", "replace")
            c7_state["limit"] = r.headers.get("ratelimit-limit")
            c7_state["remaining"] = r.headers.get("ratelimit-remaining")
            return body, (time.perf_counter() - t) * 1000, None
    except urllib.error.HTTPError as e:
        ms = (time.perf_counter() - t) * 1000
        if e.code == 429:
            c7_state["rate_limited"] += 1
            c7_state["stopped"] = True
            return None, ms, "429 rate limited"
        c7_state["errors"] += 1
        return None, ms, f"HTTP {e.code}"
    except Exception as e:
        c7_state["errors"] += 1
        return None, (time.perf_counter() - t) * 1000, str(e)


def c7_library(pkg, version, question):
    """Pick the library like the resolve step would: top search result,
    then the listed version with the same major (exact when present)."""
    key = (pkg, version)
    if key in _c7_ids:
        return _c7_ids[key], 0.0
    body, ms, err = c7_get(f"{C7}/libs/search?" + urllib.parse.urlencode({"libraryName": pkg, "query": question}))
    if not body:
        return None, ms
    results = json.loads(body).get("results", [])
    if not results:
        _c7_ids[key] = None
        return None, ms
    pick = results[0]
    lib = pick["id"]
    versions = pick.get("versions") or []
    major = version.lstrip("v").split(".")[0]
    exact = [v for v in versions if v.lstrip("v") == version.lstrip("v")]
    same = [v for v in versions if v.lstrip("v").split(".")[0] == major]
    chosen = (exact or same or [None])[-1]
    ident = f"{lib}/{chosen}" if chosen else lib
    _c7_ids[key] = {"id": ident, "versions": versions, "exact": bool(exact), "same_major": bool(same)}
    return _c7_ids[key], ms


def run_context7(q, version):
    lib, search_ms = c7_library(q["package"], version, q["question"])
    if not lib:
        return {"error": "no library" if not c7_state["stopped"] else "rate limited", "ms": search_ms}
    body, ms, err = c7_get(f"{C7}/context?" + urllib.parse.urlencode({"libraryId": lib["id"], "query": q["question"], "type": "txt"}))
    if body is None:
        return {"error": err, "ms": search_ms + ms, "library": lib["id"]}
    ok, missing, rej = grade(body, q)
    return {"pass": ok, "missing": missing, "rejected": rej, "tokens": count(body), "ms": round(search_ms + ms), "library": lib["id"],
            "version_match": "exact" if lib["exact"] else ("same major" if lib["same_major"] else "unversioned")}


def main():
    binary, projects, out = sys.argv[1], sys.argv[2], sys.argv[3]
    with_c7 = "--context7" in sys.argv
    only = None
    if "--only" in sys.argv:
        only = set(sys.argv[sys.argv.index("--only") + 1].split(","))
    qs = json.load(open(os.path.join(HERE, "questions.json")))["questions"]
    if only:
        qs = [q for q in qs if q["id"] in only]
    # Index every project first and time it (cold, empty cache).
    index = {}
    for p in sorted({q["project"] for q in qs}):
        proj = os.path.join(projects, p)
        t = time.perf_counter()
        r = subprocess.run([binary, "index", "-C", proj, "--json"], capture_output=True, text=True)
        index[p] = {"ms": round((time.perf_counter() - t) * 1000), "report": json.loads(r.stdout) if r.returncode == 0 else r.stderr}
    rows = []
    for q in qs:
        proj = os.path.join(projects, q["project"])
        text, ms, code = run_lockdocs(binary, proj, q)
        ok, missing, rej = grade(text, q)
        # The version lockdocs answered for (first line "pkg@ver · ...").
        first = text.splitlines()[0] if text else ""
        version = first.split(" · ")[0].rsplit("@", 1)[-1] if "@" in first else ""
        row = {"id": q["id"], "project": q["project"], "package": q["package"], "version": version, "question": q["question"], "why": q["why"],
               "lockdocs": {"pass": ok and code == 0, "missing": missing, "rejected": rej, "tokens": count(text), "ms": round(ms, 1)}}
        if with_c7:
            row["context7"] = run_context7(q, version or "0")
        rows.append(row)
        print(f"{q['id']:<20} lockdocs {'PASS' if row['lockdocs']['pass'] else 'fail'} {row['lockdocs']['tokens']:>5}t {row['lockdocs']['ms']:>7.1f}ms"
              + (f" | context7 {('PASS' if row['context7'].get('pass') else row['context7'].get('error') or 'fail')} {row['context7'].get('tokens', '-')}t {row['context7'].get('ms', '-')}ms" if with_c7 else ""),
              file=sys.stderr)
    summary = summarize(rows, with_c7)
    res = {"tokenizer": TOKENIZER, "index": index, "rows": rows, "summary": summary, "context7_calls": c7_state if with_c7 else None,
           "runner": {"os": os.uname().sysname, "machine": os.uname().machine}}
    json.dump(res, open(out, "w"), indent=1)
    print(markdown(res, with_c7))


def summarize(rows, with_c7):
    def agg(key):
        rs = [r[key] for r in rows if key in r and "pass" in r[key]]
        if not rs:
            return None
        toks = sorted(x["tokens"] for x in rs)
        ms = sorted(x["ms"] for x in rs)
        return {"answered": len(rs), "passed": sum(1 for x in rs if x["pass"]), "total": len(rows),
                "median_tokens": toks[len(toks) // 2], "median_ms": ms[len(ms) // 2], "p95_ms": ms[min(len(ms) - 1, int(len(ms) * 0.95))]}
    return {"lockdocs": agg("lockdocs"), "context7": agg("context7") if with_c7 else None}


def markdown(res, with_c7):
    s = res["summary"]
    out = ["## lockdocs benchmark", "", f"Tokenizer: {res['tokenizer']}. Runner: {res['runner']['os']} {res['runner']['machine']}.", ""]
    out.append("| | correct | median tokens | median latency | p95 latency |")
    out.append("|---|---|---|---|---|")
    for k in ["lockdocs", "context7"]:
        a = s.get(k)
        if a:
            out.append(f"| {k} | {a['passed']}/{a['total']} | {a['median_tokens']} | {a['median_ms']:.0f} ms | {a['p95_ms']:.0f} ms |")
    if with_c7 and res["context7_calls"]:
        c = res["context7_calls"]
        out += ["", f"Context7 (anonymous): {c['calls']} HTTP calls, {c['rate_limited']} rate-limited (429), {c['errors']} other errors; ratelimit-limit header {c['limit']}, remaining {c['remaining']}."]
    out += ["", "| question | version | lockdocs | tokens | ms |" + (" Context7 | tokens | ms | library |" if with_c7 else ""),
            "|---|---|---|---|---|" + ("---|---|---|---|" if with_c7 else "")]
    for r in res["rows"]:
        l = r["lockdocs"]
        line = f"| {r['id']} | {r['package']}@{r['version']} | {'✅' if l['pass'] else '❌'} | {l['tokens']} | {l['ms']:.0f} |"
        if with_c7:
            c = r.get("context7", {})
            mark = '✅' if c.get('pass') else ('❌' if 'pass' in c else f"— ({c.get('error')})")
            line += f" {mark} | {c.get('tokens', '')} | {c.get('ms', '')} | {c.get('library', '')} ({c.get('version_match', '')}) |"
        out.append(line)
    out += ["", "Index build (cold cache, all direct deps):", ""]
    for p, v in res["index"].items():
        out.append(f"- {p}: {v['ms']} ms")
    return "\n".join(out)


if __name__ == "__main__":
    main()

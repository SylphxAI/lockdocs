#!/usr/bin/env python3
"""lockdocs benchmark: version-specific questions against real installs.

Usage: python3 bench/run.py <lockdocs-binary> <projects-dir> <out.json> [--context7] [--only id,id]
       [--variant name:KEY=VALUE,KEY=VALUE ...]

`--variant` adds a configuration run after `lockdocs fetch` with extra
environment (for weight sweeps on CI, e.g. `head25:LOCKDOCS_HEAD_WEIGHT=0.25`).
Questions marked `"set": "held-out"` are not used for tuning: they are written
before the ranking changes they measure and reported in their own column.
Once held-out questions are used to diagnose a miss, they join the tuning set
(`history` says when) and new held-out questions replace them.

Each question names a project (with dependencies installed at pinned versions)
and a package. An answer passes when it contains at least one string from
every `expect` group and none of the `reject` strings (case-insensitive).
Tokens are counted with tiktoken o200k_base when available, else chars/4.
Context7 is queried the way its MCP server does (library search, then
context for the question) on the anonymous tier, trying the next search result
when a library answers HTTP 404; 429s are recorded, not retried.
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
    extra = ["--tokens", os.environ["BENCH_TOKENS"]] if os.environ.get("BENCH_TOKENS") else []
    r = subprocess.run([binary, "docs", q["question"], "--pkg", q["package"], "-C", proj] + extra, capture_output=True, text=True)
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
    """Pick libraries like the resolve step would: search results in order,
    each at the listed version with the same major (exact when present).
    Returns up to three candidates; the first one that answers is used."""
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
    major = version.lstrip("v").split(".")[0]
    cands = []
    for pick in results[:3]:
        lib = pick["id"]
        versions = pick.get("versions") or []
        exact = [v for v in versions if v.lstrip("v") == version.lstrip("v")]
        same = [v for v in versions if v.lstrip("v").split(".")[0] == major]
        chosen = (exact or same or [None])[-1]
        cands.append({"id": f"{lib}/{chosen}" if chosen else lib, "base": lib, "exact": bool(exact), "same_major": bool(same)})
    _c7_ids[key] = cands
    return cands, ms


def run_context7(q, version):
    cands, search_ms = c7_library(q["package"], version, q["question"])
    if not cands:
        return {"error": "no library" if not c7_state["stopped"] else "rate limited", "ms": search_ms}
    total = search_ms
    err, lib = None, cands[0]
    # An agent whose first pick fails (HTTP 404) tries the next one; so do we.
    for lib in cands:
        for ident in dict.fromkeys([lib["id"], lib["base"]]):
            body, ms, err = c7_get(f"{C7}/context?" + urllib.parse.urlencode({"libraryId": ident, "query": q["question"], "type": "txt"}))
            total += ms
            if body is not None or err != "HTTP 404":
                break
        if body is not None or err != "HTTP 404":
            break
    if body is None:
        return {"error": err, "ms": total, "library": lib["id"]}
    ok, missing, rej = grade(body, q)
    return {"pass": ok, "missing": missing, "rejected": rej, "tokens": count(body), "ms": round(total), "library": ident,
            "version_match": "exact" if lib["exact"] and ident == lib["id"] else ("same major" if lib["same_major"] and ident == lib["id"] else "unversioned")}


VARIANTS = [
    ("keyword", "lockdocs, keyword only (BM25), package files", {"LOCKDOCS_EMBED": "0", "LOCKDOCS_NO_UPSTREAM": "1"}),
    ("hybrid", "lockdocs, hybrid (BM25 + embeddings), package files", {"LOCKDOCS_NO_UPSTREAM": "1"}),
    ("fetched", "lockdocs, hybrid + upstream docs (after `lockdocs fetch`)", {}),
]


def run_lockdocs_env(binary, proj, q, env):
    old = {k: os.environ.get(k) for k in env}
    os.environ.update(env)
    try:
        return run_lockdocs(binary, proj, q)
    finally:
        for k, v in old.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


def grader_key(q):
    """Question identity for reusing a Context7 answer: text, package and grading."""
    return (q["id"], q["question"], q["package"], json.dumps([q["expect"], q.get("reject", [])]))


def main():
    binary, projects, out = sys.argv[1], sys.argv[2], sys.argv[3]
    for i, a in enumerate(sys.argv):
        if a == "--variant":
            name, _, envs = sys.argv[i + 1].partition(":")
            env = dict(kv.split("=", 1) for kv in envs.split(",") if kv)
            VARIANTS.append((name, f"lockdocs, fetched, {envs}", env))
    with_c7 = "--context7" in sys.argv
    do_fetch = "--fetch" in sys.argv
    # Reuse Context7 answers from a previous run for identical questions on the
    # same pinned version (Context7 does not depend on lockdocs changes; this
    # saves its anonymous quota). Reused rows are counted in the output.
    c7_cache = {}
    if "--context7-cache" in sys.argv:
        prev = json.load(open(sys.argv[sys.argv.index("--context7-cache") + 1]))
        for r in prev.get("rows", []):
            # Rows from before `grading` was recorded are keyed without it and never match.
            if "pass" in r.get("context7", {}) and "grading" in r:
                c7_cache[(r["id"], r["question"], r["package"], r["grading"], r["version"])] = r["context7"]
    reused = 0
    only = None
    if "--only" in sys.argv:
        only = set(sys.argv[sys.argv.index("--only") + 1].split(","))
    qs = json.load(open(os.path.join(HERE, "questions.json")))["questions"]
    if only:
        qs = [q for q in qs if q["id"] in only]
    projs = sorted({q["project"] for q in qs})
    # Index every project first and time it (cold cache, package files only).
    index = {}
    for p in projs:
        proj = os.path.join(projects, p)
        t = time.perf_counter()
        r = subprocess.run([binary, "index", "-C", proj, "--json"], capture_output=True, text=True, env={**os.environ, "LOCKDOCS_NO_UPSTREAM": "1"})
        index[p] = {"ms": round((time.perf_counter() - t) * 1000), "report": json.loads(r.stdout) if r.returncode == 0 else r.stderr}
    variants = [v for v in VARIANTS if do_fetch or v[0] in ("keyword", "hybrid")]
    results = {}
    fetch = {}
    for name, _, env in variants:
        if name == "fetched" and not fetch:
            for p in projs:
                t = time.perf_counter()
                r = subprocess.run([binary, "fetch", "-C", os.path.join(projects, p), "--json"], capture_output=True, text=True)
                fetch[p] = {"ms": round((time.perf_counter() - t) * 1000), "report": json.loads(r.stdout) if r.returncode == 0 else r.stderr[-400:]}
        for q in qs:
            proj = os.path.join(projects, q["project"])
            text, ms, code = run_lockdocs_env(binary, proj, q, env)
            ok, missing, rej = grade(text, q)
            first = text.splitlines()[0] if text else ""
            version = first.split(" · ")[0].rsplit("@", 1)[-1] if "@" in first else ""
            results[(name, q["id"])] = ({"pass": ok and code == 0, "missing": missing, "rejected": rej, "tokens": count(text), "ms": round(ms, 1)}, version)
    main_variant = "fetched" if do_fetch else variants[-1][0]
    rows = []
    for q in qs:
        main_res, version = results[(main_variant, q["id"])]
        row = {"id": q["id"], "project": q["project"], "package": q["package"], "version": version, "question": q["question"], "why": q["why"], "line": q.get("line", ""),
               "set": q.get("set", "tuning"), "grading": grader_key(q)[3], "lockdocs": main_res, "variants": {n: results[(n, q["id"])][0] for n, _, _ in variants}}
        if with_c7:
            hit = c7_cache.get(grader_key(q) + (version,))
            if hit is not None:
                row["context7"] = dict(hit, reused=True)
                reused += 1
            else:
                row["context7"] = run_context7(q, version or "0")
        rows.append(row)
        print(f"{q['id']:<20} " + " ".join(f"{n}:{'P' if row['variants'][n]['pass'] else 'f'}" for n, _, _ in variants)
              + (f" | context7 {('PASS' if row['context7'].get('pass') else row['context7'].get('error') or 'fail')}" if with_c7 else ""), file=sys.stderr)
    summary = {n: agg([r["variants"][n] for r in rows], len(rows)) for n, _, _ in variants}
    if with_c7:
        summary["context7"] = agg([r["context7"] for r in rows if "pass" in r.get("context7", {})], len(rows))
    res = {"tokenizer": TOKENIZER, "index": index, "fetch": fetch, "rows": rows, "summary": summary, "variants": [[n, label] for n, label, _ in variants],
           "context7_calls": dict(c7_state, reused=reused) if with_c7 else None, "runner": {"os": os.uname().sysname, "machine": os.uname().machine}}
    json.dump(res, open(out, "w"), indent=1)
    print(markdown(res, with_c7))


def agg(rs, total):
    if not rs:
        return None
    toks = sorted(x["tokens"] for x in rs)
    ms = sorted(x["ms"] for x in rs)
    return {"answered": len(rs), "passed": sum(1 for x in rs if x["pass"]), "total": total,
            "median_tokens": toks[len(toks) // 2], "median_ms": ms[len(ms) // 2], "p95_ms": ms[min(len(ms) - 1, int(len(ms) * 0.95))]}


def result_of(r, key):
    return r.get("context7", {}) if key == "context7" else r["variants"].get(key, {})


def markdown(res, with_c7):
    s = res["summary"]
    cols = [(n, label) for n, label in res.get("variants", [["lockdocs", "lockdocs"]])]
    if with_c7:
        cols.append(("context7", "Context7 (anonymous API)"))
    out = [f"Tokenizer: {res['tokenizer']}. Runner: {res['runner']['os']} {res['runner']['machine']}. {len(res['rows'])} questions.", ""]
    groups = [("older", "older major"), ("newer", "newer major"), ("single", "single version"), ("held-out", "held-out")]
    in_group = lambda r, g: r.get("set") == "held-out" if g == "held-out" else r.get("line") == g
    present = [g for g in groups if any(in_group(r, g[0]) for r in res["rows"])]
    out.append("| | correct | " + " | ".join(t for _, t in present) + " | median tokens | median latency | p95 latency |")
    out.append("|---|---|" + "---|" * len(present) + "---|---|---|")
    for key, label in cols:
        a = s.get(key)
        if not a:
            continue
        subs = []
        for g, _ in present:
            rs = [r for r in res["rows"] if in_group(r, g)]
            subs.append(f"{sum(1 for r in rs if result_of(r, key).get('pass'))}/{len(rs)}")
        out.append(f"| {label} | {a['passed']}/{a['total']} | " + " | ".join(subs) + f" | {a['median_tokens']} | {a['median_ms']:.0f} ms | {a['p95_ms']:.0f} ms |")
    if with_c7 and res["context7_calls"]:
        c = res["context7_calls"]
        out += ["", f"Context7 (anonymous): {c['calls']} HTTP calls, {c['rate_limited']} rate-limited (429), {c['errors']} other errors; ratelimit-limit header {c['limit']}, remaining {c['remaining']}."
                + (f" {c['reused']} answers reused from the previous run's identical question and version (see bench/run.py --context7-cache)." if c.get("reused") else "")]
    head = "| question | version | " + " | ".join(n for n, _ in cols) + " | tokens (last lockdocs) | ms |" + (" Context7 library |" if with_c7 else "")
    out += ["", head, "|---|---|" + "---|" * len(cols) + "---|---|" + ("---|" if with_c7 else "")]
    for r in res["rows"]:
        marks = []
        for key, _ in cols:
            x = result_of(r, key)
            marks.append("✅" if x.get("pass") else ("❌" if "pass" in x else f"— ({x.get('error')})"))
        line = f"| {r['id']}{' (held-out)' if r.get('set') == 'held-out' else ''} | {r['package']}@{r['version']} | " + " | ".join(marks) + f" | {r['lockdocs']['tokens']} | {r['lockdocs']['ms']:.0f} |"
        if with_c7:
            c = r.get("context7", {})
            line += f" {c.get('library', '')} ({c.get('version_match', '')}) |"
        out.append(line)
    out += ["", "Index build per project (cold cache, all direct deps, package files):", ""]
    out.append(", ".join(f"{p} {v['ms']} ms" for p, v in res["index"].items()))
    if res.get("fetch"):
        out += ["", "`lockdocs fetch` per project (one-time; upstream docs from GitHub at the version tag):", ""]
        for p, v in res["fetch"].items():
            rep = v["report"]
            if isinstance(rep, dict):
                pk = ", ".join(f"{x['package']} {x.get('files', 0)} files" for x in rep.get("packages", []) if x.get("files"))
                out.append(f"- {p}: {v['ms']} ms ({pk or 'no upstream docs'})")
            else:
                out.append(f"- {p}: {v['ms']} ms (error)")
    return "\n".join(out)


if __name__ == "__main__":
    main()

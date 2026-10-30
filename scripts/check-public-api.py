#!/usr/bin/env python3
"""Check actual installed declarations/runtime exports through CLI and MCP.

Usage: check-public-api.py BINARY [ZOD3_PROJECT ZOD4_PROJECT [ZOD4_LATEST_PROJECT]]
Without project arguments, npm installs isolated regression projects first.
All Lockdocs calls then run offline, without a model, account, or licence.
This is separate from the benchmark corpus and its graders.
"""
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading


def run(args, **kwargs):
    return subprocess.check_output(args, text=True, timeout=120, **kwargs)


def main():
    binary = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="lockdocs-public-api-") as tmp:
        env = {k: v for k, v in os.environ.items() if k not in {
            "GH_TOKEN", "GITHUB_TOKEN", "LOCKDOCS_LICENCE_TOKEN", "LOCKDOCS_TEST_LICENCE_TOKEN"
        }}
        env.update(LOCKDOCS_OFFLINE="1", LOCKDOCS_EMBED="0", LOCKDOCS_CACHE=f"{tmp}/cache")
        versions = ["3.25.76", "4.1.5", "4.6.7"]
        if len(sys.argv) in {4, 5}:
            roots = [Path(p).resolve() for p in sys.argv[2:]]
            if len(roots) == 2:
                versions = ["3.25.76", "4.6.7"]
        else:
            roots = []
            for version in versions:
                root = Path(tmp) / version
                root.mkdir()
                (root / "package.json").write_text('{"private":true}')
                subprocess.run(["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund", f"zod@{version}"], cwd=root, check=True, timeout=120)
                roots.append(root)

        def cli(root, symbol, package="zod"):
            return json.loads(run([binary, "api", symbol, "--pkg", package, "-C", str(root), "--offline", "--json"], env=env))

        def verify(answer, root, version, symbol, package="zod"):
            assert any(p["package"] == f"zod@{version}" for p in answer["provenance"]), answer
            assert all(not p["registry_fetched"] and p["upstream"] is None for p in answer["provenance"]), answer
            if symbol == "z.email" and version == "3.25.76" and package == "zod":
                assert answer["status"] == "not_exported", answer
                assert "signature" not in answer, answer
                assert any(a["import"] == "zod/v4" and "function email(" in a["signature"] for a in answer["alternatives"]), answer
                return
            assert answer["status"] == "exact" and answer["import"] == package, answer
            assert answer["applicability"] and all(a["import"] == package for a in answer["applicability"]), answer
            source = root / "node_modules/zod" / answer["file"]
            line = source.read_text().splitlines()[answer["line"] - 1]
            if symbol == "ZodObject.strict":
                assert "strict(message?: errorUtil.ErrMessage)" in answer["signature"], answer
                assert "strict(message?: errorUtil.ErrMessage)" in line, line
            elif symbol == "z.strictObject":
                assert "strictObject" in answer["signature"] and "strictObject" in line, answer
                assert ('"strict"' if version.startswith("3.") else "core.$strict") in answer["signature"], answer
            else:
                assert answer["kind"] == "function" and "function email(" in answer["signature"] and "): ZodEmail" in answer["signature"], answer
                assert "function email(" in line, line
            for a in answer["applicability"]:
                assert (root / "node_modules/zod" / a["entrypoint"]).is_file(), a

        cases = [(roots[0], "3.25.76", "ZodObject.strict", "zod"),
                 (roots[0], "3.25.76", "z.strictObject", "zod"),
                 (roots[0], "3.25.76", "z.email", "zod"),
                 (roots[0], "3.25.76", "z.email", "zod/v4"),
                 *[(root, version, symbol, "zod") for root, version in zip(roots[1:], versions[1:])
                   for symbol in ["z.email", "z.strictObject"]]]
        for root, version, symbol, package in cases:
            verify(cli(root, symbol, package), root, version, symbol, package)
        def docs(root, package="zod", query="reject unknown keys"):
            return json.loads(run([binary, "docs", query, "--pkg", package, "-C", str(root), "--offline", "--json", "--tokens", "4000"], env=env))

        def verify_docs(answer, version, major):
            assert answer["packages"] == [f"zod@{version}"], answer
            hits = [h for h in answer["hits"] if h["kind"] != "doc"]
            assert hits and all(h["file"].startswith(f"v{major}/") for h in hits), answer
            return hits

        for root, version in zip(roots, versions):
            major = version[0]
            hits = verify_docs(docs(root), version, major)
            assert any("strict" in h["path"].lower() for h in hits[:5]), hits[:5]
            if major == "4":
                assert any(h["path"] == "strictObject" for h in hits), hits
            print(f"docs: zod@{version}: " + ", ".join(h["path"] for h in hits[:5]))
            other = "4" if major == "3" else "3"
            verify_docs(docs(root, f"zod/v{other}"), version, other)
            # Warm disk cache and exact selectors must choose the same edition.
            verify_docs(docs(root, f"npm:zod@{version}"), version, major)
            runtime = json.loads(run(["node", "-e", """
const z = require('zod'), v4 = require('zod/v4');
const result = {email: typeof z.email, alternate: typeof v4.email};
if (z.email) result.validates = z.email().safeParse('ok@example.com').success && !z.email().safeParse('bad').success;
else { const r = z.object({}).strict('custom message').safeParse({extra: true}); result.message = r.error.issues[0].message; }
console.log(JSON.stringify(result));
"""], cwd=root))
            assert runtime["alternate"] == "function", runtime
            if version == "3.25.76":
                assert runtime["email"] == "undefined" and runtime["message"] == "custom message", runtime
            else:
                assert runtime["email"] == "function" and runtime["validates"], runtime
            assert f"zod@{version}" in run([binary, "resolve", "-C", str(root), "--offline"], env=env)
            assert run([binary, "docs", "schema", "--pkg", "zod", "-C", str(root), "--offline"], env=env).strip()
            print(f"runtime + CLI: zod@{version} passed")

        next_root = Path(env.get("LOCKDOCS_NEXT_PROJECT", f"{tmp}/next14")).resolve()
        if "LOCKDOCS_NEXT_PROJECT" not in env:
            next_root.mkdir()
            (next_root / "package.json").write_text('{"private":true}')
            subprocess.run(["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund", "next@14.2.35", "react@18.3.1", "react-dom@18.3.1"], cwd=next_root, check=True, timeout=120)
        assert run(["node", "-e", "console.log(typeof require('next/headers').cookies)"], cwd=next_root).strip() == "function"

        def verify_legacy_docs(answer):
            assert answer["packages"] == ["next@14.2.35"], answer
            assert any(h["path"] == "cookies" and h["file"] == "dist/client/components/headers.d.ts" for h in answer["hits"]), answer

        def verify_legacy_api(answer):
            assert answer["status"] == "exact" and answer["import"] == "next/headers", answer
            assert answer["signature"] == "export declare function cookies(): ReadonlyRequestCookies", answer
            assert answer["file"] == "dist/client/components/headers.d.ts", answer
            line = (next_root / "node_modules/next" / answer["file"]).read_text().splitlines()[answer["line"] - 1]
            assert "cookies(): ReadonlyRequestCookies" in line, line

        for package in ["next", "next/headers"]:
            verify_legacy_docs(docs(next_root, package, "cookies"))
        verify_legacy_api(cli(next_root, "cookies", "next/headers"))
        print("legacy npm: next@14.2.35 package docs and next/headers CLI passed")

        proc = subprocess.Popen([binary, "mcp", "--offline"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
        messages = queue.Queue()
        def read():
            for line in proc.stdout:
                messages.put(json.loads(line))
        threading.Thread(target=read, daemon=True).start()
        seq = 0
        def send(method, params=None, notify=False):
            nonlocal seq
            seq += 1
            request = {"jsonrpc": "2.0", "method": method}
            if not notify:
                request["id"] = seq
            if params is not None:
                request["params"] = params
            proc.stdin.write(json.dumps(request) + "\n")
            proc.stdin.flush()
            if notify:
                return
            while True:
                response = messages.get(timeout=120)
                if response.get("id") == seq:
                    assert "error" not in response, response
                    return response["result"]
        try:
            send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "public-api-check", "version": "1"}})
            send("notifications/initialized", notify=True)
            assert {t["name"] for t in send("tools/list")["tools"]} == {"resolve", "docs", "api"}
            for root, version, symbol, package in cases:
                result = send("tools/call", {"name": "api", "arguments": {"symbol": symbol, "package": package, "root": str(root), "offline": True, "format": "json"}})
                assert not result.get("isError"), result
                answer = json.loads(result["content"][0]["text"])
                verify(answer, root, version, symbol, package)
            for root, version in zip(roots, versions):
                result = send("tools/call", {"name": "docs", "arguments": {"query": "reject unknown keys", "package": "zod", "root": str(root), "offline": True, "format": "json", "tokens": 4000}})
                assert not result.get("isError"), result
                verify_docs(json.loads(result["content"][0]["text"]), version, version[0])
            for name, args in [("resolve", {}), ("docs", {"query": "schema", "package": "zod"})]:
                result = send("tools/call", {"name": name, "arguments": dict(args, root=str(roots[0]), offline=True)})
                assert not result.get("isError") and result["content"], result
            for package in ["next", "next/headers"]:
                result = send("tools/call", {"name": "docs", "arguments": {"query": "cookies", "package": package, "root": str(next_root), "offline": True, "format": "json", "tokens": 4000}})
                assert not result.get("isError"), result
                verify_legacy_docs(json.loads(result["content"][0]["text"]))
            result = send("tools/call", {"name": "api", "arguments": {"symbol": "cookies", "package": "next/headers", "root": str(next_root), "offline": True, "format": "json"}})
            assert not result.get("isError"), result
            verify_legacy_api(json.loads(result["content"][0]["text"]))
            print("MCP: exactly three tools; offline resolve/docs/api and all public API cases passed")
        finally:
            proc.terminate()
            proc.wait(timeout=10)


if __name__ == "__main__":
    main()

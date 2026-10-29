#!/usr/bin/env python3
"""Check actual installed declarations/runtime exports through CLI and MCP.

Usage: check-public-api.py BINARY [ZOD3_PROJECT ZOD4_PROJECT]
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
        env = dict(os.environ, LOCKDOCS_OFFLINE="1", LOCKDOCS_EMBED="0", LOCKDOCS_CACHE=f"{tmp}/cache")
        if len(sys.argv) == 4:
            roots = [Path(p).resolve() for p in sys.argv[2:]]
        else:
            roots = []
            for version in ["3.25.76", "4.6.7"]:
                root = Path(tmp) / version
                root.mkdir()
                (root / "package.json").write_text('{"private":true}')
                subprocess.run(["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund", f"zod@{version}"], cwd=root, check=True, timeout=120)
                roots.append(root)

        def cli(root, symbol, package="zod"):
            return json.loads(run([binary, "api", symbol, "--pkg", package, "-C", str(root), "--offline", "--json"], env=env))

        def verify(answer, root, version, symbol, package="zod"):
            assert any(p["package"] == f"zod@{version}" for p in answer["provenance"]), answer
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
            else:
                assert answer["kind"] == "function" and "function email(" in answer["signature"] and "): ZodEmail" in answer["signature"], answer
                assert "function email(" in line, line
            for a in answer["applicability"]:
                assert (root / "node_modules/zod" / a["entrypoint"]).is_file(), a

        cases = [(roots[0], "3.25.76", "ZodObject.strict", "zod"),
                 (roots[0], "3.25.76", "z.email", "zod"),
                 (roots[0], "3.25.76", "z.email", "zod/v4"),
                 (roots[1], "4.6.7", "z.email", "zod")]
        for root, version, symbol, package in cases:
            verify(cli(root, symbol, package), root, version, symbol, package)
        for root, version in zip(roots, ["3.25.76", "4.6.7"]):
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
            for name, args in [("resolve", {}), ("docs", {"query": "schema", "package": "zod"})]:
                result = send("tools/call", {"name": name, "arguments": dict(args, root=str(roots[0]), offline=True)})
                assert not result.get("isError") and result["content"], result
            print("MCP: exactly three tools; offline resolve/docs/api and all public API cases passed")
        finally:
            proc.terminate()
            proc.wait(timeout=10)


if __name__ == "__main__":
    main()

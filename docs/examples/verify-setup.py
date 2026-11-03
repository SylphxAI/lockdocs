#!/usr/bin/env python3
"""Check the setup page against published Lockdocs 0.7.0, not a source build.

Needs Python 3.11+, Node.js 18+ and npm. Uses a temporary home and project.
Initial npm installs need a connection; MCP tools run with downloads disabled.
Does not launch editor UIs or modify your real client configurations.
"""

import json
import os
from pathlib import Path
import queue
import re
import shlex
import subprocess
import tempfile
import threading
import tomllib

PACKAGE = "@sylphx/lockdocs@0.7.0"
PAGE = Path(__file__).resolve().parents[1] / "guide/setup.md"
CLIENTS = {
    "Claude Code": ("claude-code", ".claude.json", "mcpServers"),
    "Codex": ("codex", ".codex/config.toml", "mcp_servers"),
    "Cursor": ("cursor", ".cursor/mcp.json", "mcpServers"),
    "VS Code": ("vscode", ".config/Code/User/mcp.json", "servers"),
    "VS Code Insiders": ("vscode-insiders", ".config/Code - Insiders/User/mcp.json", "servers"),
    "Claude Desktop": ("claude-desktop", ".config/Claude/claude_desktop_config.json", "mcpServers"),
    "Windsurf": ("windsurf", ".codeium/windsurf/mcp_config.json", "mcpServers"),
    "Gemini CLI": ("gemini", ".gemini/settings.json", "mcpServers"),
}


def run(args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def snippet(text, title):
    if title == "VS Code Insiders":
        title = "VS Code"
    section = text.split(f"## {title}\n", 1)[1].split("\n## ", 1)[0]
    language, body = re.search(r"```(bash|json|toml)\n(.*?)\n```", section, re.S).groups()
    if language == "bash":
        command = shlex.split(body.split(" -- ", 1)[1])
        return {"type": "stdio", "command": command[0], "args": command[1:]}
    data = json.loads(body) if language == "json" else tomllib.loads(body)
    key = "servers" if title == "VS Code" else "mcp_servers" if title == "Codex" else "mcpServers"
    return data[key]["lockdocs"]


def check_mcp(entry, env):
    proc = subprocess.Popen([entry["command"], *entry["args"]], env=env,
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True)
    messages = queue.Queue()

    def read_lines():
        for line in proc.stdout:
            messages.put(json.loads(line))

    threading.Thread(target=read_lines, daemon=True).start()

    def request(number, method, params=None):
        msg = {"jsonrpc": "2.0", "id": number, "method": method}
        if params is not None:
            msg["params"] = params
        proc.stdin.write(json.dumps(msg) + "\n")
        proc.stdin.flush()
        while True:
            response = messages.get(timeout=45)
            if response.get("id") == number:
                assert "error" not in response, response
                return response["result"]

    try:
        result = request(1, "initialize", {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "setup-check", "version": "1"}})
        assert result["serverInfo"]["version"] == "0.7.0", result
        proc.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        proc.stdin.flush()
        result = request(2, "tools/list")
        assert {t["name"] for t in result["tools"]} == {"resolve", "docs", "api"}, result
        result = request(3, "tools/call", {"name": "resolve", "arguments": {}})
        assert "zod@4.1.5" in json.dumps(result), result
        result = request(4, "tools/call", {
            "name": "api", "arguments": {"symbol": "ZodObject.strict", "package": "zod", "tokens": 300}})
        assert "v4/classic/schemas.d.ts:427" in json.dumps(result), result
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        proc.stdin.close()
        proc.stdout.close()
        proc.stderr.close()


def main():
    text = PAGE.read_text()
    with tempfile.TemporaryDirectory(prefix="lockdocs-setup-check-") as temp:
        root = Path(temp)
        home = root / "home"
        home.mkdir()
        project = root / "project"
        project.mkdir()
        env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"),
                   LOCKDOCS_ROOT=str(project), LOCKDOCS_CACHE=str(root / "cache"),
                   LOCKDOCS_OFFLINE="1", LOCKDOCS_NO_SYSTEM_PYTHON="1")
        # No real user configuration or npm registry credentials are needed.
        env["NPM_CONFIG_USERCONFIG"] = str(root / "empty-npmrc")
        run(["npm", "install", "--prefix", str(root / "runner"), "--ignore-scripts",
             "--no-audit", "--no-fund", PACKAGE], env=env)
        run(["npm", "init", "-y"], cwd=project, env=env, stdout=subprocess.DEVNULL)
        run(["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund",
             "--save-exact", "zod@4.1.5"], cwd=project, env=env)
        binary = root / "runner/node_modules/.bin/lockdocs"
        run([str(binary), "version"], env=env)
        run([str(binary), "setup", "--client", ",".join(c[0] for c in CLIENTS.values())], env=env)
        for title, (_, path, key) in CLIENTS.items():
            content = (home / path).read_text()
            generated = (tomllib.loads(content) if path.endswith(".toml") else json.loads(content))[key]["lockdocs"]
            documented = snippet(text, title)
            assert documented["command"] == "npx" and documented["args"] == ["-y", PACKAGE, "mcp"], documented
            # setup deliberately tracks current npm; manual examples pin the tested release.
            normalized = dict(documented, args=["-y", "@sylphx/lockdocs", "mcp"])
            assert generated == normalized, (title, generated, normalized)
            check_mcp(documented, env)
            print(f"PASS {title}: config, initialize, tools/list, resolve, api", flush=True)


if __name__ == "__main__":
    main()

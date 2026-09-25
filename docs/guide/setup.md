# Editors and agents

`npx -y @sylphx/lockdocs setup` writes these for you. To do it by hand:

| Client | Config | Entry |
|---|---|---|
| Claude Code | `claude mcp add --scope user lockdocs -- npx -y @sylphx/lockdocs mcp` | |
| Codex | `~/.codex/config.toml` | `[mcp_servers.lockdocs]` `command = "npx"`, `args = ["-y", "@sylphx/lockdocs", "mcp"]` |
| Cursor | `~/.cursor/mcp.json` | `mcpServers.lockdocs` |
| VS Code | `<config>/Code/User/mcp.json` | `servers.lockdocs` with `"type": "stdio"` |
| Claude Desktop | `<config>/Claude/claude_desktop_config.json` | `mcpServers.lockdocs` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers.lockdocs` |
| Gemini CLI | `~/.gemini/settings.json` | `mcpServers.lockdocs` |

`<config>` is `~/.config` on Linux, `~/Library/Application Support` on macOS and `%APPDATA%` on Windows. On Windows the command is `cmd /c npx -y @sylphx/lockdocs mcp`.

## Environment

| Variable | Effect |
|---|---|
| `LOCKDOCS_ROOT` | Project directory when the client sends no roots |
| `LOCKDOCS_FETCH=1` | Allow [fetching](./fetch) exact versions that are not installed |
| `LOCKDOCS_CACHE` | Cache directory (default: the OS cache dir + `/lockdocs`) |
| `LOCKDOCS_NO_SYSTEM_PYTHON=1` | Do not ask the system interpreter for its site-packages |

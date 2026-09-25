# Quickstart

## Add it to your agent

```bash
npx -y @sylphx/lockdocs setup
```

`setup` detects Claude Code, Codex, Cursor, VS Code, VS Code Insiders, Claude Desktop, Windsurf and Gemini CLI, writes each one's MCP config, and prints every change. It is idempotent. `--dry-run` shows what it would do, `--remove` undoes it, `--client cursor,codex` limits it, and `--fetch` registers the server with [fetching](./fetch) enabled.

Restart the client, then ask:

> Use lockdocs: which version of zod do we use, and how do I reject unknown keys?

The agent calls `resolve`, then `docs` with `package: "zod"`, and gets the answer for your version, cited to the installed file.

## Use it from a terminal

Inside any project with dependencies installed:

```bash
npx -y @sylphx/lockdocs resolve                       # every pinned version and whether its docs are here
npx -y @sylphx/lockdocs zod "reject unknown keys"     # docs for the pinned zod
npx -y @sylphx/lockdocs api tokio::spawn              # exact signature + doc comment
npx -y @sylphx/lockdocs zod                           # overview: key API and README
```

For a faster start, install it: `npm i -g @sylphx/lockdocs`, then run `lockdocs ...`.

## Manual MCP config

```json
{
  "mcpServers": {
    "lockdocs": { "command": "npx", "args": ["-y", "@sylphx/lockdocs", "mcp"] }
  }
}
```

The server answers for the client's workspace root (from MCP roots), else `LOCKDOCS_ROOT`, else its working directory. Every tool also accepts `root`.

import { createSignal, For } from "solid-js";
import { CodeExample } from "@/components/editorial";
import "./editor-mcp-tabs.css";

const editors = ["Cursor", "Claude Code"] as const;
type Editor = typeof editors[number];

const cursorConfig = JSON.stringify({
  mcpServers: { lockdocs: { command: "npx", args: ["-y", "@sylphx/lockdocs", "mcp"] } },
}, null, 2);
const claudeSetup = "claude mcp add --scope local --transport stdio lockdocs -- npx -y @sylphx/lockdocs mcp\nclaude mcp list";

export function EditorMcpTabs() {
  const [editor, setEditor] = createSignal<Editor>("Cursor");

  return <div class="editor-mcp">
    <div class="segmented" role="group" aria-label="MCP editor setup">
      <For each={editors}>{name =>
        <button type="button" aria-pressed={editor() === name} onClick={() => setEditor(name)}>{name}</button>
      }</For>
    </div>
    <div class="editor-mcp-panels">
      <div class="editor-mcp-panel" aria-hidden={editor() !== "Cursor"} inert={editor() !== "Cursor"}>
        <p>Save this as <code>.cursor/mcp.json</code> in your project. If the file exists, add only the <code>lockdocs</code> entry under <code>mcpServers</code> so you keep your other servers.</p>
        <CodeExample label="Cursor project configuration" code={cursorConfig}/>
        <p>Reload Cursor and check that <code>lockdocs</code> is enabled in its MCP settings.</p>
      </div>
      <div class="editor-mcp-panel" aria-hidden={editor() !== "Claude Code"} inert={editor() !== "Claude Code"}>
        <p>Run this from your project folder. <code>--scope local</code> keeps the registration private to you in this project. If a <code>lockdocs</code> entry already exists, inspect it with <code>claude mcp get lockdocs</code> before replacing it.</p>
        <CodeExample label="Claude Code project setup" code={claudeSetup}/>
        <p>Look for <code>lockdocs</code> in the list. If Claude Code is already open, start a new session and type <code>/mcp</code> to check its connection.</p>
      </div>
    </div>
  </div>;
}

import { For } from "solid-js";
import "./mcp-setup.css";

export const mcpSetups = {
  Cursor: {
    code: JSON.stringify({ mcpServers: { lockdocs: { command: "npx", args: ["-y", "@sylphx/lockdocs", "mcp"] } } }, null, 2),
    label: ".cursor/mcp.json",
    kind: "mcp_config",
  },
  "Claude Code": {
    code: "claude mcp add --scope local --transport stdio lockdocs -- npx -y @sylphx/lockdocs mcp\nclaude mcp list",
    label: "Claude Code CLI",
    kind: "cli",
  },
} as const;

export type McpEditor = keyof typeof mcpSetups;
const editors = Object.keys(mcpSetups) as McpEditor[];

export function McpEditorPicker(props: { label: string; editor: McpEditor; onSelect: (editor: McpEditor) => void }) {
  return <div class="segmented mcp-editor-picker" role="group" aria-label={props.label}>
    <For each={editors}>{name =>
      <button type="button" aria-pressed={props.editor === name} onClick={() => props.onSelect(name)}>{name}</button>
    }</For>
  </div>;
}

import { A } from "@solidjs/router";
import { createSignal, Show } from "solid-js";
import { ArrowUpRight, Braces } from "lucide-solid";
import { CodeExample } from "@/components/editorial";
import { McpEditorPicker, mcpSetups, type McpEditor } from "@/components/mcp-setup";
import { editorGuidePath } from "@/lib/editorial-routes";
import { useSiteLanguage } from "@/lib/site-language";

export function PlaygroundMcp() {
  const { text, carryCampaign } = useSiteLanguage();
  const [open, setOpen] = createSignal(false);
  const [editor, setEditor] = createSignal<McpEditor>("Cursor");

  return <section class="playground-mcp surface">
    <button type="button" class="mcp-drawer-toggle" aria-expanded={open()} onClick={() => setOpen(!open())}>
      <Braces size={18} aria-hidden="true"/> {text("MCP agent setup", "MCP 代理工具設定")} <span aria-hidden="true">{open() ? "−" : "+"}</span>
    </button>
    <Show when={open()}>
      <div class="mcp-drawer-content">
        <p class="quiet">{text("Run your agent from the project root so Lockdocs can find the lockfile.", "請從專案根目錄啟動代理工具，Lockdocs 才能找到 lockfile。")}</p>
        <McpEditorPicker label={text("MCP editor setup", "MCP 編輯器設定")} editor={editor()} onSelect={setEditor}/>
        <Show when={mcpSetups[editor()]} keyed>{setup =>
          <CodeExample label={setup.label} code={setup.code} copyLabel={text("Copy ", "複製 ") + setup.label}
            analytics={{ surface: "playground_mcp", command_kind: setup.kind }}/>
        }</Show>
        <A class="mcp-guide-link" href={carryCampaign(editorGuidePath)}>
          {text("Cursor & Claude Code setup guide", "Cursor 與 Claude Code 設定指南（英文）")} <ArrowUpRight size={14} aria-hidden="true"/>
        </A>
      </div>
    </Show>
  </section>;
}

import { createSignal, For, Show } from "solid-js";
import { ArrowUpRight, Terminal } from "lucide-solid";
import { CopyButton } from "@/components/copy-button";
import { pypiUrl } from "@/lib/install";
import "./install-command.css";

const options = [
  { tool: "npx", command: "npx -y @sylphx/lockdocs setup" },
  { tool: "uvx", command: "uvx lockdocs setup" },
  { tool: "pip", command: "pip install lockdocs" },
] as const;

export function InstallCommand() {
  const [selected, setSelected] = createSignal<(typeof options)[number]>(options[0]);
  return <section class="install-card" aria-label="Install Lockdocs">
    <div class="install-tools">
      <div class="install-options" role="radiogroup" aria-label="Install method">
        <For each={options}>{option => <label>
          <input class="sr-only" type="radio" name="install-method" value={option.tool}
            checked={selected() === option} onChange={() => setSelected(option)}/>
          <span>{option.tool}</span>
        </label>}</For>
      </div>
      <a href={pypiUrl}>PyPI <ArrowUpRight size={12} aria-hidden="true"/></a>
    </div>
    <div class="install-command">
      <Terminal size={16} aria-hidden="true"/>
      <div class="install-code">
        <For each={options}>{option => <code classList={{ active: selected() === option }}>{option.command}</code>}</For>
      </div>
      <Show when={selected().command} keyed>{command => <CopyButton value={command} label="Copy install command"/>}</Show>
    </div>
  </section>;
}

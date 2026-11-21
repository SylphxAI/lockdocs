

import { createMemo, createSignal, For, Show } from "solid-js";
import { A as Link } from "@solidjs/router";
import { ArrowRight, ArrowUpRight, Braces, FileCode2, GitCompareArrows, ListTree } from "lucide-solid";
import { PackageText } from "@/components/package-text";
import { CopyButton } from "@/components/copy-button";
import { formats, samples, resolveLockfile, sampleCitation, upgrades, type Dependency, type Format } from "@/lib/playground";

const snippet = JSON.stringify({ mcpServers: { lockdocs: { command: "npx", args: ["-y", "@sylphx/lockdocs", "mcp"] } } }, null, 2);
const keyFor = (row: Dependency) => [row.ecosystem, row.name, row.version, row.source].join(":");

export function Studio() {
  const [format, setFormat] = createSignal<Format>(formats[0]);
  const [input, setInput] = createSignal(samples[formats[0]]);
  const [formatUsed, setFormatUsed] = createSignal<Format>(formats[0]);
  const initialRows = resolveLockfile(formats[0], samples[formats[0]]);
  const [rows, setRows] = createSignal(initialRows);
  const [selected, setSelected] = createSignal<Dependency | null>(initialRows[0] ?? null);
  const [message, setMessage] = createSignal("");
  const [count, setCount] = createSignal(35);
  const [upgrade, setUpgrade] = createSignal<keyof typeof upgrades>("next");
  const [snippetOpen, setSnippetOpen] = createSignal(false);
  const active = createMemo(() => upgrades[upgrade()]);
  function run(nextFormat: Format, text: string) {
    try {
      const resolved = resolveLockfile(nextFormat, text);
      setRows(resolved); setSelected(resolved[0] ?? null); setCount(35); setFormatUsed(nextFormat);
      setMessage(resolved.length ? "" : "No exact package versions found in this input. Try a pinned sample.");
    } catch {
      setRows([]); setSelected(null); setMessage("Couldn’t parse this input. Check the selected format and use a valid lockfile under 400,000 characters.");
    }
  }
  function submit(event: SubmitEvent) { event.preventDefault(); run(format(), input()); }
  function loadSample(next: Format) { setFormat(next); setInput(samples[next]); run(next, samples[next]); }
  return <>
    <div class="studio-grid surface">
      <form onSubmit={submit} class="studio-input"><div class="panel-title"><FileCode2 size={17}/> <label for="lockfile">Lockfile input</label></div>
        <div class="input-tools"><select aria-label="Lockfile format" value={format()} onChange={(event) => setFormat(event.currentTarget.value as Format)}><For each={formats}>{(option) => <option>{option}</option>}</For></select><button type="button" class="button secondary small" onClick={() => loadSample(format())}>Load sample</button></div>
        <textarea id="lockfile" value={input()} onInput={(event) => setInput(event.currentTarget.value)} spellcheck={false} maxlength={400000}/>
        <div class="inline-actions"><button class="button primary small" type="submit">Resolve versions <ArrowRight size={15}/></button><span class="quiet mono">Runs locally</span></div>
      </form>
      <section class="studio-output"><div class="panel-title"><ListTree size={17}/> Resolved packages <span class="panel-count" aria-live="polite">{rows().length} pinned</span></div>
        {message() && <p class="notice" role="status">{message()}</p>}
        <ul class="package-list"><For each={rows().slice(0,count())}>{(row) => <li><button type="button" aria-pressed={selected() ? keyFor(selected()!) === keyFor(row) : false} onClick={() => setSelected(row)}><strong>{row.name}</strong><span>{row.version}</span><small>{row.ecosystem}</small></button></li>}</For></ul>
        {rows().length > count() && <button class="button secondary small" type="button" onClick={() => setCount(count()+35)}>Show more packages</button>}
        <Show when={selected()} keyed>{(item) => <div class="inspect"><div class="plan-label">DOC LOOKUP</div><h3><PackageText text={`${item.name}@${item.version}`}/></h3><p class="quiet">Pinned in <code>{formatUsed()}</code>{item.source === "git" && " · Git source"}.</p>{sampleCitation(item) ? <p class="citation"><PackageText text={sampleCitation(item) ?? ""}/></p> : <p class="quiet">No bundled citation for this version. Run the CLI in your project to search the installed files.</p>}<code>lockdocs {item.name} --offline</code></div>}</Show>
      </section>
    </div>
    <section class="snippet-box surface"><button type="button" class="snippet-toggle" aria-expanded={snippetOpen()} onClick={() => setSnippetOpen(!snippetOpen())}><Braces size={18}/> MCP agent setup <span>{snippetOpen() ? "−" : "+"}</span></button><Show when={snippetOpen()}><><p class="quiet">Run your agent from the project root so Lockdocs sees its lockfile.</p><pre>{snippet}</pre><CopyButton value={snippet}/></></Show></section>
    <section class="upgrade-section" id="upgrade"><div class="page-intro"><div class="kicker"><span/> PRO / REPORT PREVIEW</div><h2>See exactly what breaks.</h2><p>Explore two representative call sites. These examples use release citations; paste a lockfile above to inspect versions, not to scan your own source code.</p></div>
      <div class="segmented" aria-label="Upgrade example"><For each={Object.entries(upgrades)}>{([id, item]) => <button type="button" aria-pressed={id === upgrade()} onClick={() => setUpgrade(id as keyof typeof upgrades)}>{item.label}</button>}</For></div>
      <article class="surface upgrade-card"><div class="panel-title"><GitCompareArrows size={17}/><code>{active().command}</code></div><div class="upgrade-body"><h3>{active().change}</h3><p class="quiet">Example call site <code>{active().path}</code></p><div class="sample-diff"><div><pre>- {active().before}</pre><p><PackageText text={active().oldSource}/></p></div><div><pre>+ {active().after}</pre><p><PackageText text={active().newSource}/></p></div></div><p>{active().note}</p><For each={active().links}>{([label,href]) => <a class="doc-source" href={href}>{label} <ArrowUpRight size={14}/></a>}</For></div></article>
      <div class="upgrade-cta"><div><strong>Get the full report with Pro.</strong><p>Upgrade Reports for every affected call site, plus Private Package Docs.</p></div><Link href="/pricing" class="button primary">See Pro plans <ArrowRight size={16}/></Link></div>
    </section>
  </>;
}

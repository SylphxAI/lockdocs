import { createMemo, createSignal, For, Show } from "solid-js";
import { A as Link } from "@solidjs/router";
import { ArrowRight, ArrowUpRight, Braces, FileCode2, GitCompareArrows, ListTree } from "lucide-solid";
import { PackageText } from "@/components/package-text";
import { CopyButton } from "@/components/copy-button";
import { useCookieSettings, useProImpression } from "@/components/cookie-settings";
import { samples, resolveLockfile, sampleCitation, sampleExcerpt, upgrades, type Dependency } from "@/lib/playground";
import { formats, type Format } from "@/lib/lockfile-formats";
import { useSiteLanguage } from "@/lib/site-language";

type ExampleId = keyof typeof upgrades;
const exampleIds = Object.keys(upgrades) as ExampleId[];
const initialFormat = formats[0];
const initialSample = samples[initialFormat];
const snippet = JSON.stringify({ mcpServers: { lockdocs: { command: "npx", args: ["-y", "@sylphx/lockdocs", "mcp"] } } }, null, 2);
const keyFor = (row: Dependency) => [row.ecosystem, row.name, row.version, row.source].join(":");

export function Studio() {
  const { campaignHref, text } = useSiteLanguage();
  const { trackEvent } = useCookieSettings();
  const upgradeImpression = useProImpression("playground_upgrade_cta");
  const reportImpression = useProImpression("playground_report_preview");
  const [format, setFormat] = createSignal<Format>(initialFormat);
  const [formatUsed, setFormatUsed] = createSignal<Format>(initialFormat);
  const initialRows = resolveLockfile(initialFormat, initialSample);
  const [rows, setRows] = createSignal(initialRows);
  const [selected, setSelected] = createSignal<Dependency | null>(initialRows[0] ?? null);
  const [message, setMessage] = createSignal<"empty" | "invalid">();
  const [count, setCount] = createSignal(35);
  const [upgrade, setUpgrade] = createSignal<ExampleId>("next");
  const [snippetOpen, setSnippetOpen] = createSignal(false);
  const active = createMemo(() => upgrades[upgrade()]);
  let editor!: HTMLTextAreaElement;
  let usingSample = true;

  function isSample() { return usingSample && editor.value === samples[format()]; }
  function resolve(nextFormat: Format, input: string, sourceKind: "sample" | "pasted") {
    let resolved: Dependency[];
    try { resolved = resolveLockfile(nextFormat, input); }
    catch { setRows([]); setSelected(null); setMessage("invalid"); return; }
    setRows(resolved);
    setSelected(resolved[0] ?? null);
    setCount(35);
    setFormatUsed(nextFormat);
    setMessage(resolved.length ? undefined : "empty");
    trackEvent("lockfile_resolved", { lockfile_format: nextFormat, source_kind: sourceKind, package_count: resolved.length });
  }
  function submit(event: SubmitEvent) {
    event.preventDefault();
    resolve(format(), editor.value, isSample() ? "sample" : "pasted");
  }
  function loadSample(next: Format) {
    setFormat(next);
    editor.value = samples[next];
    usingSample = true;
    resolve(next, editor.value, "sample");
  }
  function changeFormat(next: Format) {
    // Switch bundled examples immediately, but never replace the visitor’s edits.
    if (isSample()) loadSample(next);
    else { usingSample = false; setFormat(next); }
  }
  function switchExample(id: ExampleId) {
    if (id === upgrade()) return;
    setUpgrade(id);
    trackEvent("upgrade_preview_switched", { example_id: id });
  }

  return <>
    <div class="studio-grid surface">
      <form onSubmit={submit} class="studio-input"><div class="panel-title"><FileCode2 size={17}/> <label for="lockfile">{text("Lockfile input", "輸入 lockfile")}</label></div>
        <div class="input-tools"><select aria-label={text("Lockfile format", "lockfile 格式")} value={format()} onChange={event => changeFormat(event.currentTarget.value as Format)}><For each={formats}>{option => <option>{option}</option>}</For></select><button type="button" class="button secondary small" onClick={() => loadSample(format())}>{text("Load sample", "載入範例")}</button></div>
        {/* HTML uses the textarea's contents, not a value attribute, for its initial value. */}
        <textarea id="lockfile" ref={editor} onInput={() => { usingSample = false; }} spellcheck={false} maxlength={400000}>{initialSample}</textarea>
        <div class="inline-actions"><button class="button primary small" type="submit">{text("Resolve versions", "查出套件版本")} <ArrowRight size={15}/></button><span class="quiet mono">{text("Runs locally", "在本機執行")}</span></div>
      </form>
      <section class="studio-output"><div class="panel-title"><ListTree size={17}/> {text("Resolved packages", "找到的套件")} <span class="panel-count" aria-live="polite">{text(rows().length + " pinned", "已鎖定 " + rows().length + " 個套件")}</span></div>
        <Show when={message()}>{kind => <p class="notice" role="status">{kind() === "empty"
          ? text("No exact package versions found in this input. Try a pinned sample.", "找不到指定確切版本的套件。可以先載入範例試試看。")
          : text("Couldn’t parse this input. Check the selected format and use a valid lockfile under 400,000 characters.", "無法讀取這份 lockfile。請確認選對格式，檔案內容正確，且不超過 40 萬個字元。")}</p>}</Show>
        <ul class="package-list"><For each={rows().slice(0,count())}>{(row) => <li><button type="button" aria-pressed={selected() ? keyFor(selected()!) === keyFor(row) : false} onClick={() => setSelected(row)}><strong>{row.name}</strong><span>{row.version}</span><small>{row.ecosystem}</small></button></li>}</For></ul>
        {rows().length > count() && <button class="button secondary small" type="button" onClick={() => setCount(count()+35)}>{text("Show more packages", "顯示更多套件")}</button>}
        <Show when={selected()} keyed>{item => {
          const command = `lockdocs ${item.name} --offline`;
          return <div class="inspect">
          <div class="plan-label">{text("Bundled citation preview", "內建引用範例")}</div><h3><PackageText text={item.name + "@" + item.version}/></h3>
          <p class="quiet">{text("Pinned in ", "版本鎖定於 ")}<code>{formatUsed()}</code>{item.source === "git" && text(" · Git source", " · 來源為 Git")}{text(".", "。")}</p>
          <Show when={sampleExcerpt(item)} fallback={<>{sampleCitation(item) ? <p class="citation"><PackageText text={sampleCitation(item) ?? ""}/></p> : <p class="quiet">{text("No bundled citation for this version. Run the CLI in your project to search the installed files.", "此版本沒有內建的引用範例。請在專案中執行 CLI，搜尋已安裝的檔案。")}</p>}</>}>
            {excerpt => <><pre class="sample-source"><code>{excerpt().code}</code></pre><a class="doc-source" href={excerpt().url}>{excerpt().cite} <ArrowUpRight size={14}/></a></>}
          </Show><div class="studio-command">
            <code>{command}</code>
            <CopyButton value={command} label={text("Copy CLI command", "複製 CLI 指令")} analytics={{ surface: "playground_cli", command_kind: "cli" }}/>
          </div>
        </div>;}}</Show>
      </section>
    </div>
    <section class="surface upgrade-cta" aria-labelledby="pro-upgrade-preview" ref={upgradeImpression}>
      <div>
        <h2 id="pro-upgrade-preview">{text("Pro Upgrade Preview", "Pro 升級預覽")}</h2>
        <p>{text("Free resolves the exact versions in your lockfile. In your project, Pro Upgrade Reports show which call sites an upgrade affects, with citations. Pro also includes docs for private packages.", "免費版可查出 lockfile 裡的確切版本。Pro 升級報告會找出專案中受升級影響的呼叫位置，並附上原始碼出處。Pro 也能查閱私有套件的文件。")}</p>
      </div>
      <Link href={campaignHref("/pricing")} class="button secondary small" onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "playground_upgrade_cta" })}>{text("See Pro plans", "查看 Pro 方案")} <ArrowRight size={16}/></Link>
    </section>
    <section class="snippet-box surface">
      <button type="button" class="snippet-toggle" aria-expanded={snippetOpen()} onClick={() => setSnippetOpen(!snippetOpen())}><Braces size={18}/> {text("MCP agent setup", "MCP 代理工具設定")} <span aria-hidden="true">{snippetOpen() ? "−" : "+"}</span></button>
      <Show when={snippetOpen()}><p class="quiet">{text("Run your agent from the project root so Lockdocs can find the lockfile.", "請從專案根目錄啟動代理工具，Lockdocs 才能找到 lockfile。")}</p><pre>{snippet}</pre><CopyButton value={snippet} analytics={{ surface: "playground_mcp", command_kind: "mcp_config" }}/></Show>
    </section>
    <section class="upgrade-section" id="upgrade" aria-labelledby="report-preview-title">
      <div class="page-intro" ref={reportImpression}><div class="kicker"><span/> {text("PRO / REPORT PREVIEW", "PRO / 升級報告預覽")}</div><h2 id="report-preview-title">{text("See exactly what breaks.", "升級後，哪些程式碼需要改？")}</h2><p>{text("Compare two example call sites with citations from each release. Pasting a lockfile above checks versions only; it doesn’t scan your source code.", "比較兩個呼叫範例，查看各版本的原始碼出處。上方貼上的 lockfile 只用來查版本，不會掃描你的程式碼。")}</p></div>
      <div class="segmented" role="group" aria-label={text("Upgrade example", "升級範例")}><For each={exampleIds}>{id => <button type="button" aria-pressed={id === upgrade()} onClick={() => switchExample(id)}>{upgrades[id].label}</button>}</For></div>
      <article class="surface upgrade-card">
        <div class="panel-title studio-command"><GitCompareArrows size={17}/><code>{active().command}</code>
          <Show when={active().command} keyed>{command => <CopyButton value={command} label={text("Copy upgrade command", "複製升級指令")} analytics={{ surface: "playground_upgrade", command_kind: "cli" }}/>}</Show>
        </div>
        <div class="upgrade-body"><h3>{text(active().change[0], active().change[1])}</h3><p class="quiet">{text("Example call site: ", "呼叫位置範例：")}<code>{active().path}</code></p>
          <div class="sample-diff"><div><pre>- {active().before}</pre><p><PackageText text={active().oldSource}/></p></div><div><pre>+ {active().after}</pre><p><PackageText text={active().newSource}/></p></div></div>
          <p>{text(active().note[0], active().note[1])}</p><a class="doc-source" href={active().guideUrl}>{text(active().guide[0], active().guide[1])} <ArrowUpRight size={14}/></a>
        </div>
      </article>
      <Link href={campaignHref("/pricing")} class="doc-source" onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "playground_report_preview" })}>{text("Get full reports with Pro", "用 Pro 取得完整升級報告")} <ArrowRight size={14}/></Link>
    </section>
  </>;
}

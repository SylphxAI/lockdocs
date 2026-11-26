import { createMemo, createSignal, For, Show } from "solid-js";
import { A as Link, useLocation } from "@solidjs/router";
import { ArrowRight, ArrowUpRight, GitCompareArrows, ListTree, Search } from "lucide-solid";
import { PlaygroundMcp } from "@/components/playground-mcp";
import { StudioInput } from "@/components/studio-input";
import { PackageText } from "@/components/package-text";
import { CopyButton } from "@/components/copy-button";
import { useCookieSettings, useProImpression } from "@/components/cookie-settings";
import { resolveLockfile, sampleCitation, sampleExcerpt, type Dependency } from "@/lib/playground";
import { filterPackages, matchingUpgrade, offlinePackageCommand } from "@/lib/playground-results";
import type { EventParams } from "@/lib/analytics-events";
import { upgradeExample, upgradeIds, upgrades, type UpgradeId } from "@/lib/upgrade-examples";
import { samples, upgradeSample } from "@/lib/playground-samples";
import { formats, type Format } from "@/lib/lockfile-formats";
import { previewLimit } from "@/lib/playground-file";
import { useSiteLanguage } from "@/lib/site-language";

const pageSize = 35;
const parseErrorId = "lockfile-parse-error";

export function Studio() {
  const initialUpgrade = upgradeExample(useLocation().search);
  const sample = initialUpgrade ? upgradeSample(initialUpgrade) : undefined;
  const initialFormat = sample?.format ?? formats[0];
  const initialInput = sample?.input ?? samples[initialFormat];
  const initialRows = resolveLockfile(initialFormat, initialInput);
  const initialSelection = initialRows.find(row => row.name + "@" + row.version === sample?.pin);
  const { proHref, text } = useSiteLanguage();
  const { trackEvent } = useCookieSettings();
  const upgradeImpression = useProImpression("playground_upgrade_cta");
  const reportImpression = useProImpression("playground_report_preview");
  const [formatUsed, setFormatUsed] = createSignal<Format>(initialFormat);
  const [rows, setRows] = createSignal(initialRows);
  const [selected, setSelected] = createSignal<Dependency | null>(initialSelection ?? initialRows[0] ?? null);
  const [message, setMessage] = createSignal<"empty" | "invalid" | "oversize">();
  const [count, setCount] = createSignal(pageSize);
  const [query, setQuery] = createSignal("");
  const visibleRows = createMemo(() => filterPackages(rows(), query()));
  const inspected = createMemo(() => {
    const current = selected();
    return current && visibleRows().includes(current) ? current : visibleRows()[0];
  });
  const isInspected = (row: Dependency) => inspected() === row;
  let packageList!: HTMLUListElement;

  function updateQuery(next: string) {
    setQuery(next);
    setCount(pageSize);
    packageList.scrollTop = 0;
  }
  const [upgrade, setUpgrade] = createSignal<UpgradeId>(initialUpgrade ?? "next");
  const active = createMemo(() => upgrades[upgrade()]);

  function clearInputError() {
    if (message() === "invalid" || message() === "oversize") setMessage(undefined);
  }

  function resolve(nextFormat: Format, input: string, sourceKind: EventParams<"lockfile_resolved">["source_kind"]) {
    updateQuery("");
    let resolved: Dependency[];
    try { resolved = resolveLockfile(nextFormat, input); }
    catch {
      setRows([]);
      setSelected(null);
      setMessage(input.length > previewLimit ? "oversize" : input.trim() ? "invalid" : "empty");
      return;
    }
    setRows(resolved);
    setSelected(resolved[0] ?? null);
    setFormatUsed(nextFormat);
    setMessage(resolved.length ? undefined : "empty");
    const example = sourceKind !== "sample" && matchingUpgrade(resolved);
    if (example) setUpgrade(example);
    trackEvent("lockfile_resolved", { lockfile_format: nextFormat, source_kind: sourceKind, package_count: resolved.length });
  }
  function switchExample(id: UpgradeId) {
    if (id === upgrade()) return;
    setUpgrade(id);
    trackEvent("upgrade_preview_switched", { example_id: id }, { requireExplicitConsent: true });
  }

  return <>
    <div class="studio-grid surface">
      <StudioInput initialFormat={initialFormat} initialInput={initialInput} resolve={resolve}
        errorId={message() === "invalid" ? parseErrorId : undefined} onEdit={clearInputError}/>
      <section class="studio-output">
        <div class="panel-title"><ListTree size={17}/> {text("Resolved packages", "找到的套件")}
          <span class="panel-count" aria-live="polite">{query().trim()
            ? text(visibleRows().length + " of " + rows().length + " pinned", "符合 " + visibleRows().length + " / " + rows().length + " 個套件")
            : text(rows().length + " pinned", "已鎖定 " + rows().length + " 個套件")}</span>
        </div>
        <Show when={rows().length > 5}><div class="package-filter">
          <Search size={16} aria-hidden="true"/>
          <input type="search" aria-label={text("Filter packages", "篩選套件")} placeholder={text("Filter packages…", "篩選套件…")}
            value={query()} onInput={event => updateQuery(event.currentTarget.value)}/>
        </div></Show>
        <Show when={message()}>{kind => <p class="notice" role="status" id={kind() === "invalid" ? parseErrorId : undefined}>{kind() === "empty"
          ? text("No exact package versions found in this input. Try a pinned sample.", "找不到指定確切版本的套件。可以先載入範例試試看。")
          : text("Couldn’t parse this input. Check the selected format and use a valid lockfile under 400,000 characters.", "無法讀取這份 lockfile。請確認選對格式，檔案內容正確，且不超過 40 萬個字元。")}</p>}</Show>
        <Show when={query().trim() && !visibleRows().length}>
          <p class="notice">{text("No matching packages", "找不到符合的套件")}</p>
        </Show>
        <ul ref={packageList} class="package-list"><For each={visibleRows().slice(0, count())}>{row => <li><button type="button" aria-pressed={isInspected(row)} onClick={() => setSelected(row)}><strong>{row.name}</strong><span>{row.version}</span><small>{row.ecosystem}</small></button></li>}</For></ul>
        {visibleRows().length > count() && <button class="button secondary small" type="button" onClick={() => setCount(count() + pageSize)}>{text("Show more packages", "顯示更多套件")}</button>}
        <Show when={inspected()} keyed>{item => {
          const command = offlinePackageCommand(item);
          return <div class="inspect">
          <div class="plan-label">{text("Bundled citation preview", "內建引用範例")}</div><h3><PackageText text={item.name + "@" + item.version}/></h3>
          <p class="quiet">{text("Pinned in ", "版本鎖定於 ")}<code>{formatUsed()}</code>{item.source === "git" && text(" · Git source", " · 來源為 Git")}{text(".", "。")}</p>
          <Show when={sampleExcerpt(item)} fallback={<>{sampleCitation(item) ? <p class="citation"><PackageText text={sampleCitation(item) ?? ""}/></p> : <p class="quiet">{text("No bundled citation for this version. Run the CLI in your project to search the installed files.", "此版本沒有內建的引用範例。請在專案中執行 CLI，搜尋已安裝的檔案。")}</p>}</>}>
            {excerpt => <><pre class="sample-source"><code>{excerpt().code}</code></pre><a class="doc-source" href={excerpt().url}>{excerpt().cite} <ArrowUpRight size={14}/></a></>}
          </Show>
          <p class="quiet">{text(
            "Run from your project root after installing its dependencies. Use Node.js 18+; npx may download Lockdocs before the offline query.",
            "請先安裝專案的相依套件，再到專案根目錄執行。需使用 Node.js 18 以上；離線查詢前，npx 可能需要先下載 Lockdocs。",
          )} <Link href="/guide#install-lockdocs" class="underline underline-offset-2">{text("CLI install options", "CLI 安裝方式（英文）")}</Link></p>
          <div class="studio-command">
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
      <Link href={proHref("playground_upgrade_cta")} class="button secondary small" onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "playground_upgrade_cta" })}>{text("See Pro plans", "查看 Pro 方案")} <ArrowRight size={16}/></Link>
    </section>
    <PlaygroundMcp/>
    <section class="upgrade-section" id="upgrade" aria-labelledby="report-preview-title">
      <div class="page-intro" ref={reportImpression}><div class="kicker"><span/> {text("PRO / REPORT PREVIEW", "PRO / 升級報告預覽")}</div><h2 id="report-preview-title">{text("See exactly what breaks.", "升級後，哪些程式碼需要改？")}</h2><p>{text("Compare call sites before and after an upgrade, with source citations from each release. The lockfile preview checks versions only; it doesn’t scan your source code.", "比較升級前後的呼叫範例，並查看各版本的原始碼出處。上方的 lockfile 預覽只用來查版本，不會掃描你的程式碼。")}</p></div>
      <div class="segmented" role="group" aria-label={text("Upgrade example", "升級範例")}><For each={upgradeIds}>{id => <button type="button" aria-pressed={id === upgrade()} onClick={() => switchExample(id)}>{upgrades[id].label}</button>}</For></div>
      <article class="surface upgrade-card">
        <div class="panel-title studio-command"><GitCompareArrows size={17}/><code>{active().command}</code>
          <Show when={active().command} keyed>{command => <CopyButton value={command} label={text("Copy upgrade command", "複製升級指令")} analytics={{ surface: "playground_upgrade", command_kind: "cli" }}/>}</Show>
        </div>
        <div class="upgrade-body"><h3>{text(active().change[0], active().change[1])}</h3><p class="quiet">{text("Example call site: ", "呼叫位置範例：")}<code>{active().path}</code></p>
          <div class="sample-diff"><div><pre>- {active().before}</pre><p><PackageText text={active().oldSource}/></p></div><div><pre>+ {active().after}</pre><p><PackageText text={active().newSource}/></p></div></div>
          <p>{text(active().note[0], active().note[1])}</p><a class="doc-source" href={active().guideUrl}>{text(active().guide[0], active().guide[1])} <ArrowUpRight size={14}/></a>
        </div>
      </article>
      <Link href={proHref("playground_report_preview")} class="doc-source" onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "playground_report_preview" })}>{text("Get full reports with Pro", "用 Pro 取得完整升級報告")} <ArrowRight size={14}/></Link>
    </section>
  </>;
}

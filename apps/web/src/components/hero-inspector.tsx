import { A } from "@solidjs/router";
import { ArrowRight, FileCode2, GitCompareArrows } from "lucide-solid";
import { createMemo, createSignal, For } from "solid-js";
import { PackageText } from "@/components/package-text";
import { upgradeSample } from "@/lib/playground-samples";
import { useSiteLanguage } from "@/lib/site-language";
import { upgradeIds, upgradePreviewPath, upgrades, type UpgradeId } from "@/lib/upgrade-examples";
import "./hero-inspector.css";

export function HeroInspector() {
  const { carryCampaign, href, text } = useSiteLanguage();
  const [selected, select] = createSignal<UpgradeId>("next");
  const example = () => upgrades[selected()];
  const sample = createMemo(() => upgradeSample(selected()));
  return <section class="hero-preview" aria-labelledby="version-inspector-title">
    <header class="preview-chrome">
      <h2 id="version-inspector-title"><FileCode2 size={15} aria-hidden="true"/> {text("VERSION INSPECTOR", "版本檢視器")}</h2>
      <span class="preview-live"><span class="signal"/> {text("SAMPLE PROJECT", "範例專案")}</span>
    </header>
    <div class="preview-switcher" role="group" aria-label={text("Upgrade ecosystem", "升級套件")}>
      <For each={upgradeIds}>{id =>
        <button type="button" aria-pressed={selected() === id} onClick={() => select(id)}>{upgrades[id].name}</button>
      }</For>
    </div>
    <A href={carryCampaign(upgradePreviewPath(href("/playground"), selected()))} class="preview-link">
      {text(`Inspect the ${example().name} upgrade`, `查看 ${example().name} 升級報告`)} <ArrowRight size={15} aria-hidden="true"/>
    </A>
    <div class="preview-files">
      <code>{sample().format}</code><code class="mint" aria-live="polite"><PackageText text={sample().pin}/></code>
    </div>
    <pre class="preview-lock"><code><PackageText text={sample().snippet}/></code></pre>
    <div class="preview-result">
      <h3 class="preview-label"><GitCompareArrows size={14} aria-hidden="true"/> {text("UPGRADE DIFF", "升級差異")} <code>{example().path}</code></h3>
      <div class="preview-diff before">
        <span aria-label={text("Before", "升級前")}>−</span><code>{example().before}</code>
        <small><PackageText text={example().oldSource}/></small>
      </div>
      <div class="preview-diff after">
        <span aria-label={text("After", "升級後")}>+</span><code>{example().after}</code>
        <small><PackageText text={example().newSource}/></small>
      </div>
    </div>
  </section>;
}

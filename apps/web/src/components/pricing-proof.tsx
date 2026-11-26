import { A } from "@solidjs/router";
import { ArrowRight } from "lucide-solid";
import { createSignal, For } from "solid-js";
import { useCookieSettings } from "@/components/cookie-settings";
import { PackageText } from "@/components/package-text";
import { useSiteLanguage } from "@/lib/site-language";
import { upgradeIds, upgradePreviewPath, upgrades, type UpgradeId } from "@/lib/upgrade-examples";
import { privateProofExamples } from "@/lib/private-proof-examples";
import { proProofPaths } from "@/lib/pricing-faq";
import "./pricing-proof.css";

type Registry = keyof typeof privateProofExamples;
const registryIds = Object.keys(privateProofExamples) as Registry[];

export function PricingDecisionProof() {
  const { text } = useSiteLanguage();
  const example = upgrades.next;
  return <div class="pricing-decision-proof">
    <div class="plan-label">{text("Upgrade report example", "升級報告範例")}</div>
    <p><strong>{example.label}</strong> · {text(example.change[0], example.change[1])}<br/>
      <code>{example.path}</code>
    </p>
  </div>;
}

export function PricingProof() {
  const { text, locale, href, carryCampaign } = useSiteLanguage();
  const { trackEvent } = useCookieSettings();
  const paths = () => proProofPaths(locale());
  const [upgrade, setUpgrade] = createSignal<UpgradeId>("next");
  const [registry, setRegistry] = createSignal<Registry>("npm");
  const example = () => upgrades[upgrade()];
  const privateExample = () => privateProofExamples[registry()];
  return <section class="pricing-proof" aria-labelledby="pricing-proof-title">
    <header>
      <h2 id="pricing-proof-title">{text("What Pro shows you", "Pro 功能實例")}</h2>
      <span>{text("EXAMPLES", "操作範例")}</span>
    </header>
    <div class="pricing-proof-grid">
      <article>
        <h3 id="pricing-proof-upgrade" class="plan-label">{text("01 / UPGRADE REPORT", "01 / 升級報告")}</h3>
        <div class="pricing-proof-switcher upgrade-options" role="group" aria-labelledby="pricing-proof-upgrade">
          <For each={upgradeIds}>{id =>
            <button type="button" aria-pressed={id === upgrade()} onClick={() => setUpgrade(id)}>{upgrades[id].label}</button>
          }</For>
        </div>
        <p aria-live="polite">{text(example().change[0], example().change[1])}</p>
        <p class="pricing-proof-path"><code>{example().path}</code></p>
        <div class="sample-diff">
          <div><pre><code>- {example().before}</code></pre><p><PackageText text={example().oldSource}/></p></div>
          <div><pre><code>+ {example().after}</code></pre><p><PackageText text={example().newSource}/></p></div>
        </div>
        <A class="pricing-proof-link" href={carryCampaign(upgradePreviewPath(href("/playground"), upgrade()))}
          onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "pricing_proof_upgrade_report" }, { requireExplicitConsent: true })}>{text("Try the report preview", "互動預覽升級報告")} <ArrowRight size={15} aria-hidden="true"/></A>
      </article>
      <article>
        <h3 id="pricing-proof-private" class="plan-label">{text("02 / PRIVATE PACKAGE DOCS", "02 / 私有套件文件")}</h3>
        <div class="pricing-proof-switcher" role="group" aria-labelledby="pricing-proof-private">
          <For each={registryIds}>{id =>
            <button type="button" aria-pressed={id === registry()} onClick={() => setRegistry(id)}>{privateProofExamples[id].label}</button>
          }</For>
        </div>
        <p>{text("Fetch docs for a pinned internal package, then search them offline.", "先下載指定版本的內部套件文件，再離線查閱。")}</p>
        <div class="pricing-proof-terminal">
          <pre><code>{privateExample().fetch}</code></pre>
          <pre><code>{privateExample().search}</code></pre>
        </div>
        <p class="quiet"><span aria-live="polite">{text(privateExample().setup[0], privateExample().setup[1])}</span>{" "}{text("Credentials go only to their configured origin; your project code stays local.", "憑證只會傳到設定的來源，專案程式碼仍留在本機。")}</p>
        <A class="pricing-proof-link" href={carryCampaign(paths().privateSources)}
          onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "pricing_proof_private_sources" }, { requireExplicitConsent: true })}>{text("Read the private registry setup", "查看私有套件庫設定（英文）")} <ArrowRight size={15} aria-hidden="true"/></A>
      </article>
    </div>
  </section>;
}

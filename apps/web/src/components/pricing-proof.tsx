import { A } from "@solidjs/router";
import { ArrowRight } from "lucide-solid";
import { PackageText } from "@/components/package-text";
import { useSiteLanguage } from "@/lib/site-language";
import { upgrades } from "@/lib/upgrade-examples";
import { proProofPaths } from "@/lib/pricing-faq";
import "./pricing-proof.css";

const example = upgrades.next;

export function PricingProof() {
  const { text, locale, carryCampaign } = useSiteLanguage();
  const paths = () => proProofPaths(locale());
  return <section class="pricing-proof" aria-labelledby="pricing-proof-title">
    <header>
      <h2 id="pricing-proof-title">{text("What Pro shows you", "Pro 功能實例")}</h2>
      <span>{text("EXAMPLES", "操作範例")}</span>
    </header>
    <div class="pricing-proof-grid">
      <article>
        <h3 class="plan-label">{text("01 / UPGRADE REPORT", "01 / 升級報告")}</h3>
        <p>{text(example.change[0], example.change[1])}</p>
        <p class="pricing-proof-path"><code>{example.path}</code></p>
        <div class="sample-diff">
          <div><pre><code>- {example.before}</code></pre><p><PackageText text={example.oldSource}/></p></div>
          <div><pre><code>+ {example.after}</code></pre><p><PackageText text={example.newSource}/></p></div>
        </div>
        <A class="pricing-proof-link" href={carryCampaign(paths().preview)}>{text("Try the report preview", "互動預覽升級報告")} <ArrowRight size={15} aria-hidden="true"/></A>
      </article>
      <article>
        <h3 class="plan-label">{text("02 / PRIVATE PACKAGE DOCS", "02 / 私有套件文件")}</h3>
        <p>{text("Fetch docs for a pinned internal npm package, then search them offline.", "先下載鎖定版本的內部 npm 套件文件，再離線查閱。")}</p>
        <div class="pricing-proof-terminal">
          <pre><code>lockdocs fetch @acme/session</code></pre>
          <pre><code>{'lockdocs docs "session renewal" --pkg @acme/session --offline'}</code></pre>
        </div>
        <p class="quiet">{text("Uses your existing .npmrc. Credentials go only to their configured origin; your project code stays local.", "沿用現有的 .npmrc 設定。憑證只會傳到設定的來源，專案程式碼仍留在本機。")}</p>
        <A class="pricing-proof-link" href={carryCampaign(paths().privateSources)}>{text("Read the private registry setup", "查看私有套件庫設定（英文）")} <ArrowRight size={15} aria-hidden="true"/></A>
      </article>
    </div>
  </section>;
}

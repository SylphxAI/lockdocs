import { A } from "@solidjs/router";
import { ArrowRight } from "lucide-solid";
import { For } from "solid-js";
import { useCookieSettings } from "@/components/cookie-settings";
import type { ArticleSurface } from "@/lib/analytics-events";
import { useSiteLanguage } from "@/lib/site-language";
import { usdProOffer } from "@/lib/storefront-schema";
import { upgradeIds, upgradePreviewPath, upgrades } from "@/lib/upgrade-examples";

const annualPrice = "US$" + new Intl.NumberFormat("en-US").format(usdProOffer.price);

export function ArticleBridge(props: { surface: ArticleSurface }) {
  const { campaignHref, carryCampaign, href, proHref, text } = useSiteLanguage();
  const { trackEvent } = useCookieSettings();
  return <section class="surface article-bridge" aria-labelledby="article-next-step">
    <div class="eyebrow">{text("NEXT STEP", "下一步")}</div>
    <h2 id="article-next-step">{text("Try it with a lockfile", "用 lockfile 試試看")}</h2>
    <p>{text(
      "Check pinned versions in the free playground or look up docs with the local CLI. Pro adds call-site upgrade reports and docs from private package registries.",
      "用免費工具確認套件的鎖定版本，或用本機 CLI 查閱文件。Pro 支援查閱私有套件文件，升級報告也會標出受影響的呼叫位置。",
    )}</p>
    <div class="button-row">
      <A class="button secondary" href={campaignHref("/playground")}>{text("Try the playground", "開啟試用工具")} <ArrowRight size={16} aria-hidden="true"/></A>
      <A class="button outline" href={proHref(props.surface)} onClick={() => trackEvent("pro_cta_clicked", { entry_surface: props.surface })}>{text(`See Lockdocs Pro — ${annualPrice}/yr`, `查看 Lockdocs Pro — ${annualPrice}／年`)} <ArrowRight size={16} aria-hidden="true"/></A>
    </div>
    <nav class="article-upgrade-links" aria-labelledby="article-upgrade-label">
      <span id="article-upgrade-label">{text("Upgrade examples", "升級範例")}</span>
      <div class="article-upgrade-row">
        <For each={upgradeIds}>{id =>
          <A href={carryCampaign(upgradePreviewPath(href("/playground"), id))}>{upgrades[id].name}</A>
        }</For>
      </div>
    </nav>
  </section>;
}

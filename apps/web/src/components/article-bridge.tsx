import { A } from "@solidjs/router";
import { ArrowRight } from "lucide-solid";
import { useCookieSettings } from "@/components/cookie-settings";
import type { ArticleSurface } from "@/lib/analytics-events";
import { useSiteLanguage } from "@/lib/site-language";
import { usdProOffer } from "@/lib/storefront-schema";

const annualPrice = new Intl.NumberFormat("en-US").format(usdProOffer.price);

export function ArticleBridge(props: { surface: ArticleSurface }) {
  const { campaignHref } = useSiteLanguage();
  const { trackEvent } = useCookieSettings();
  return <section class="surface article-bridge" aria-labelledby="article-next-step">
    <div class="eyebrow">NEXT STEP</div>
    <h2 id="article-next-step">Try it with a lockfile</h2>
    <p>Check pinned versions in the free playground or look up docs with the local CLI. Pro adds call-site upgrade reports and docs from private package registries.</p>
    <div class="button-row">
      <A class="button secondary" href={campaignHref("/playground")}>Try the playground <ArrowRight size={16} aria-hidden="true"/></A>
      <A class="button outline" href={campaignHref("/pricing")} onClick={() => trackEvent("pro_cta_clicked", { entry_surface: props.surface })}>See Lockdocs Pro — US${annualPrice}/yr <ArrowRight size={16} aria-hidden="true"/></A>
    </div>
  </section>;
}

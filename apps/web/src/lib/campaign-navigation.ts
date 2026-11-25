import { createContext, type Accessor } from "solid-js";
import { parseSignalAttribution } from "@sylphx/sdk/signal";

const fields = ["gclid", "gbraid", "wbraid", "utm_id", "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content", "utm_source_platform", "utm_creative_format", "utm_marketing_tactic"] as const;
const base = "https://lockdocs.invalid";
export const CampaignBlocked = createContext<Accessor<boolean>>();

/** Carry campaign parameters in first-party links only. Signal owns all persistence after consent. */
export function withCampaign(href: string, search: string): string {
  if (!search || !/^\/(?![/\\])/.test(href)) return href;
  const target = new URL(href, base);
  if (target.origin !== base) return href;
  const source = new URL("/" + search, base);
  const campaign = parseSignalAttribution(source);
  if (!fields.some(key => campaign[key])) return href;
  for (const key of fields) {
    // Validate as Signal does, but preserve the original decoded value in the URL.
    if (campaign[key] && !target.searchParams.has(key)) target.searchParams.set(key, source.searchParams.get(key)!);
  }
  return target.pathname + target.search + target.hash;
}

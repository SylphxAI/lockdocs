import { checkoutCampaign } from "./checkout-campaign";

const legacyPaths = new Set(["/quickstart", "/download"]);
const campaignFields = ["utm_source", "utm_medium", "utm_campaign", "utm_id", "utm_term", "utm_content", "ref"];

export function legacyGuide(request: Request) {
  if (request.method !== "GET" && request.method !== "HEAD") return;
  const url = new URL(request.url);
  if (!legacyPaths.has(url.pathname.replace(/\/$/, ""))) return;
  const query = new URLSearchParams(checkoutCampaign(url.searchParams));
  for (const key of campaignFields) {
    const [value, ...duplicates] = url.searchParams.getAll(key);
    if (!duplicates.length && value && /^[\p{L}\p{N} ._+-]{1,200}$/u.test(value)) query.set(key, value);
  }
  return new Response(null, { status: 301, headers: { Location: query.size ? "/guide?" + query : "/guide" } });
}

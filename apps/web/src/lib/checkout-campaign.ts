export const campaignCookie = "__Host-lockdocs-checkout-campaign";
export const campaignMaxAge = 24 * 60 * 60;

export function checkoutCampaign(params: URLSearchParams) {
  const ids: Record<string, string> = {};
  for (const name of ["gclid", "gbraid", "wbraid"]) {
    const [value, ...duplicates] = params.getAll(name);
    if (!duplicates.length && value && /^[A-Za-z0-9_.-]{1,200}$/.test(value)) ids[name] = value;
  }
  return ids;
}

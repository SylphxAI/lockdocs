export const canonicalOrigin = "https://lockdocs.sylphx.com";
const live = new URL(canonicalOrigin).hostname;
function hostname(authority: string | null) {
  if (!authority || !/^[a-zA-Z0-9.-]+(?::[0-9]{1,5})?$/.test(authority)) return null;
  return authority.split(":")[0].toLowerCase();
}
// Ingress rewrites Host to the service authority and supplies the original host.
// Reject ambiguous forwarded values rather than picking a hop, and never let a
// forwarded header override a different public Host.
export function allowIndexing(headers: Headers) {
  const host = hostname(headers.get("host"));
  const forwarded = headers.get("x-forwarded-host");
  if (forwarded === null) return host === live;
  return hostname(forwarded) === live &&
    (host === live || /^[a-z0-9-]+\.[a-z0-9-]+\.svc\.cluster\.local$/.test(host ?? ""));
}

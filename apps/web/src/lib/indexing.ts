import { authorityHostname, isKnativeServiceHost } from "./public-origin";

export const canonicalOrigin = "https://lockdocs.sylphx.com";
const live = new URL(canonicalOrigin).hostname;
// Ingress rewrites Host to the service authority and supplies the original host.
// Reject ambiguous forwarded values rather than picking a hop, and never let a
// forwarded header override a different public Host.
export function allowIndexing(headers: Headers) {
  const host = authorityHostname(headers.get("host"));
  const forwarded = headers.get("x-forwarded-host");
  if (forwarded === null) return host === live;
  return authorityHostname(forwarded) === live &&
    (host === live || isKnativeServiceHost(host));
}

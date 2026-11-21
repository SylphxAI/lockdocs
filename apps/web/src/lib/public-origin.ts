const authorityPattern = /^[a-zA-Z0-9.-]+(?::[0-9]{1,5})?$/;

export function authorityHostname(authority: string | null) {
  if (!authority || !authorityPattern.test(authority)) return null;
  return authority.split(":")[0].toLowerCase();
}

export function isKnativeServiceHost(host: string | null) {
  return /^[a-z0-9-]+\.[a-z0-9-]+\.svc\.cluster\.local$/.test(host ?? "");
}

function hostForProtocol(authority: string, protocol: string) {
  if (!authorityHostname(authority)) return null;
  try { return new URL(protocol + "//" + authority).host; }
  catch { return null; }
}

// This is a target-origin check, not the browser Origin header check used for
// CSRF. Ingress rewrites Host to its Knative service authority. Trust only a
// single HTTPS forwarding pair stamped by the managed literal-host Gateway
// rule, never from another public Host. The configured origin owns redirects.
export function hasPublicOrigin(request: Request, expected: string): boolean {
  const target = new URL(expected);
  if (target.protocol !== "https:" || target.origin !== expected) return false;
  const url = new URL(request.url);
  if (url.protocol !== "http:" && url.protocol !== "https:") return false;
  const host = request.headers.get("host");
  if (host !== null && hostForProtocol(host, url.protocol) !== url.host) return false;

  const forwardedHost = request.headers.get("x-forwarded-host");
  const forwardedProto = request.headers.get("x-forwarded-proto");
  if (forwardedHost === null && forwardedProto === null) return url.origin === expected;
  if (forwardedProto !== "https" || forwardedHost === null ||
      hostForProtocol(forwardedHost, "https:") !== target.host) return false;
  return host !== null && (hostForProtocol(host, "https:") === target.host || isKnativeServiceHost(url.hostname));
}

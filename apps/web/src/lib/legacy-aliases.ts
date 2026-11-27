/** Retired public paths and the canonical page each one moved to. */
export const legacyAliases = new Map([["/guide/quickstart", "/guide"], ["/reference/cli", "/reference"], ["/recover", "/dashboard"]]);
// Shipped CLIs print /recover as their licence-recovery link, so browsers must not cache that redirect for good.
const uncached = new Set(["/recover"]);

/** Redirect a page load straight to the canonical page. The query is dropped, as it was before. */
export function legacyAlias(request: Request) {
  if (request.method !== "GET" && request.method !== "HEAD") return;
  const path = new URL(request.url).pathname.replace(/\/$/, "");
  const location = legacyAliases.get(path);
  if (!location) return;
  const headers: Record<string, string> = { Location: location };
  if (uncached.has(path)) headers["Cache-Control"] = "private, no-store";
  return new Response(null, { status: 308, headers });
}

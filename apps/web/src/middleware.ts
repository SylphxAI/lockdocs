import { createMiddleware } from "@solidjs/start/middleware";
import { legacySignIn } from "@/lib/auth-entry";
import { legacyGuide } from "@/lib/guide-redirects";
import { allowIndexing } from "@/lib/indexing";
import { chinesePaths, pagePath } from "@/lib/locales";
export default createMiddleware({ onRequest: (event) => {
  if (!allowIndexing(event.request.headers)) event.response.headers.set("X-Robots-Tag", "noindex, nofollow");
  event.response.headers.set("X-Content-Type-Options", "nosniff");
  const { pathname: path, search, searchParams } = new URL(event.request.url);
  // The initial Stripe return must not send the query as a Referer, even before hydration.
  const checkoutReturn = pagePath(path) === "/dashboard" && searchParams.has("session_id");
  event.response.headers.set("Referrer-Policy", checkoutReturn ? "no-referrer" : "strict-origin-when-cross-origin");
  if (checkoutReturn) event.response.headers.set("Cache-Control", "private, no-store");
  event.response.headers.set("X-Frame-Options", path === "/api/analytics/tag" ? "SAMEORIGIN" : "DENY");
  if (event.request.method === "GET" || event.request.method === "HEAD") {
    if (path === "/privacy" || path === "/privacy/") {
      return new Response(null, { status: 308, headers: { Location: "/legal/privacy" } });
    }
    const normalized = pagePath(path);
    const canonical = normalized.replace(/^\/zh-Hant(?=\/|$)/, "/zh-TW");
    if (canonical !== normalized && chinesePaths.includes(canonical)) {
      return new Response(null, { status: 308, headers: { Location: canonical + search } });
    }
  }
  return legacyGuide(event.request) ?? legacySignIn(event.request);
} });

import { createMiddleware } from "@solidjs/start/middleware";
import { legacySignIn } from "@/lib/auth-entry";
import { legacyGuide } from "@/lib/guide-redirects";
import { allowIndexing } from "@/lib/indexing";
import { chinesePaths, pagePath } from "@/lib/locales";
export default createMiddleware({ onRequest: (event) => {
  if (!allowIndexing(event.request.headers)) event.response.headers.set("X-Robots-Tag", "noindex, nofollow");
  event.response.headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  event.response.headers.set("X-Content-Type-Options", "nosniff");
  const { pathname: path, search } = new URL(event.request.url);
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

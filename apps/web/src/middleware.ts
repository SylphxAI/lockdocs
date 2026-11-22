import { createMiddleware } from "@solidjs/start/middleware";
import { legacySignIn } from "@/lib/auth-entry";
import { allowIndexing } from "@/lib/indexing";
export default createMiddleware({ onRequest: (event) => {
  if (!allowIndexing(event.request.headers)) event.response.headers.set("X-Robots-Tag", "noindex, nofollow");
  event.response.headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  event.response.headers.set("X-Content-Type-Options", "nosniff");
  const path = new URL(event.request.url).pathname;
  event.response.headers.set("X-Frame-Options", path === "/api/analytics/tag" ? "SAMEORIGIN" : "DENY");
  if ((event.request.method === "GET" || event.request.method === "HEAD") && (path === "/privacy" || path === "/privacy/")) {
    return new Response(null, { status: 308, headers: { Location: "/legal/privacy" } });
  }
  return legacySignIn(event.request);
} });

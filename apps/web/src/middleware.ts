import { createMiddleware } from "@solidjs/start/middleware";
import { allowIndexing } from "@/lib/indexing";
export default createMiddleware({ onRequest: (event) => {
  if (!allowIndexing(event.request.headers)) event.response.headers.set("X-Robots-Tag", "noindex, nofollow");
  event.response.headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  event.response.headers.set("X-Content-Type-Options", "nosniff");
  event.response.headers.set("X-Frame-Options", "DENY");
} });

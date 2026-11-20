import type { APIEvent } from "@solidjs/start/server";
import { allowIndexing } from "@/lib/indexing";
export function GET({request}: APIEvent) {
  return new Response(allowIndexing(request.headers) ? "User-agent: *\nAllow: /\n" : "User-agent: *\nDisallow: /\n", { headers: { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "no-store" } });
}

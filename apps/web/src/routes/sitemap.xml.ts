import type { APIEvent } from "@solidjs/start/server";
import { docPages } from "@/lib/docs";
import { allowIndexing, canonicalOrigin } from "@/lib/indexing";

const paths = ["/", "/pricing", "/playground", "/legal/privacy", "/legal/terms", ...docPages.map(({ route }) => route)];

export function GET({ request }: APIEvent) {
  const urls = allowIndexing(request.headers) ? paths.map(path => `<url><loc>${canonicalOrigin}${path}</loc></url>`).join("\n") : "";
  return new Response(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`, {
    headers: { "Content-Type": "application/xml; charset=utf-8", "Cache-Control": "no-store" },
  });
}

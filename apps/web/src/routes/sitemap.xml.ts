import { docPages } from "@/lib/docs";
import { canonicalOrigin } from "@/lib/indexing";

const paths = ["/", "/pricing", "/playground", "/legal/privacy", "/legal/terms", ...docPages.map(({ route }) => route)];

export function GET() {
  const urls = paths.map(path => `<url><loc>${canonicalOrigin}${path}</loc></url>`).join("\n");
  return new Response(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`, {
    headers: { "Content-Type": "application/xml; charset=utf-8", "Cache-Control": "no-store" },
  });
}

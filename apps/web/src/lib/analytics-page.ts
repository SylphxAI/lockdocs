import { docPages } from "./docs";
import { context7AlternativePath, editorGuidePath } from "./editorial-routes";

// Report only known routes. A catch-all pathname can contain private data just
// as easily as a query string. Never use a URL or title supplied by the visitor.
const titles = new Map<string, string>([
  ["/", "Lockdocs — Docs for the version you installed"],
  ["/pricing", "Pricing · Lockdocs"],
  ["/playground", "Lockfile Playground · Lockdocs"],
  ["/zh-TW", "Lockdocs — 查閱已安裝版本的文件"],
  ["/zh-TW/docs", "文件總覽 · Lockdocs"],
  ["/zh-TW/playground", "lockfile 試用工具 · Lockdocs"],
  ["/zh-TW/pricing", "價格 · Lockdocs"],
  ["/dashboard", "Account · Lockdocs"],
  ["/sign-in", "Sign in · Lockdocs"],
  ["/cli/authorize", "Authorize the CLI · Lockdocs"],
  ["/legal/privacy", "Privacy · Lockdocs"],
  ["/legal/terms", "Terms of service · Lockdocs"],
  [context7AlternativePath, "Context7 alternative for exact-version docs · Lockdocs"],
  [editorGuidePath, "Up-to-date docs for Cursor and Claude Code · Lockdocs"],
  ...docPages.map(({ route, title }): [string, string] => [route, title + " · Lockdocs"]),
]);

function publicPage(raw: string, origin: string) {
  if (!raw) return null;
  try {
    const url = new URL(raw);
    if ((url.protocol !== "https:" && url.protocol !== "http:") || url.origin !== origin || url.username || url.password) return null;
    const path = url.pathname.replace(/\/+$/, "") || "/";
    const title = titles.get(path);
    return title ? { location: url.origin + path, title } : null;
  } catch { return null; }
}

export function analyticsPage(location: string, referrer: string, origin: string) {
  const page = publicPage(location, origin);
  return page && {
    page_location: page.location,
    page_title: page.title,
    page_referrer: publicPage(referrer, origin)?.location ?? "",
  };
}

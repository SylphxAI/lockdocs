import { A, useLocation } from "@solidjs/router";
import { createMemo, createSignal, For, Show } from "solid-js";
import { BookOpen, Search } from "lucide-solid";
import { docGroups, docPages, type DocPage } from "@/lib/docs";
import { pagePath } from "@/lib/locales";
import { useSiteLanguage } from "@/lib/site-language";

const titles = {
  "/docs": "文件總覽", "/guide": "快速入門", "/guide/setup": "設定程式開發代理工具",
  "/guide/how-it-works": "運作方式", "/guide/ecosystems": "支援的語言與套件管理工具",
  "/guide/fetch": "下載文件", "/reference": "CLI 參考", "/reference/tools": "MCP 工具",
  "/benchmarks": "評測結果", "/compare": "工具比較", "/guide/docs-site-mcp": "文件網站 MCP 伺服器", "/capabilities": "功能與限制",
  "/vision": "設計方向", "/pro": "Pro 功能參考",
} satisfies Record<DocPage["route"], string>;
const groups: Record<string, string> = { Introduction: "介紹", Guide: "指南", Reference: "參考", Project: "專案" };

export function DocNavigation() {
  const { href, text, locale } = useSiteLanguage();
  const location = useLocation();
  const [expanded, setExpanded] = createSignal(false);
  const [query, setQuery] = createSignal("");
  const title = (page: DocPage) => text(page.title, titles[page.route]);
  const terms = createMemo(() => query().trim().toLowerCase().split(/\s+/).filter(Boolean));
  const results = createMemo(() => docPages.filter(page => {
    const searchable = [page.title, titles[page.route], page.description, page.route].join(" ").toLowerCase();
    return terms().every(term => searchable.includes(term));
  }));
  const open = () => expanded() || terms().length > 0;
  const filtered = () => terms().length > 0;
  return <aside class="docs-sidebar">
    <form class="docs-search" role="search" onSubmit={event => event.preventDefault()}>
      <label for="docs-query">{text("Find a guide", "搜尋指南")}</label>
      <div><Search size={16} aria-hidden="true"/><input type="search" id="docs-query" placeholder={text("MCP, upgrade", "MCP、下載")} value={query()} onInput={event => setQuery(event.currentTarget.value)}/></div>
    </form>
    <button type="button" class="docs-nav-toggle" aria-expanded={open()} aria-controls="docs-navigation" onClick={() => { const next = !open(); setQuery(""); setExpanded(next); }}>
      <BookOpen size={16}/>{open() ? text("Hide documentation navigation", "收合文件導覽") : text("Browse documentation", "展開文件導覽")}
    </button>
    <nav id="docs-navigation" classList={{ "docs-nav-open": open() }} aria-label={text("Documentation", "文件導覽")}>
      <Show when={filtered()} fallback={<>
        <A class="docs-brand" href={href("/docs")}><BookOpen size={16}/> {text("Lockdocs docs", "Lockdocs 文件")}</A>
        <For each={docGroups}>{group => <div class="docs-group"><div>{text(group.label, groups[group.label])}</div><For each={group.pages}>{item => <A href={href(item.route)} end>{title(item)}</A>}</For></div>}</For>
      </>}>
        <p class="quiet docs-result-count" role="status">{text(
          results().length === 0 ? "No matching guides." : results().length + (results().length === 1 ? " matching guide" : " matching guides"),
          results().length === 0 ? "找不到符合的指南。" : "找到 " + results().length + " 篇指南",
        )}</p>
        <ul class="docs-group docs-results"><For each={results()}>{item => <li><A href={href(item.route)} end>{title(item)}
          {locale() === "zh-TW" && href(item.route) === item.route && <small>（英文）</small>}
        </A></li>}</For></ul>
      </Show>
    </nav>
    {pagePath(location.pathname) === "/zh-TW/docs" && <p class="docs-language-note quiet">其他技術指南目前以英文提供。</p>}
  </aside>;
}

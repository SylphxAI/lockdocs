import { A, Navigate, useLocation } from "@solidjs/router";
import { HttpStatusCode } from "@solidjs/start";
import { createMemo, Show } from "solid-js";
import { ArrowRight, BookOpen, Terminal } from "lucide-solid";
import { SolidMarkdown as Markdown } from "solid-markdown";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";
import { visit } from "unist-util-visit";
import { PackageText } from "@/components/package-text";
import { PageMeta } from "@/components/page-meta";
import { context7AlternativePath, editorGuidePath } from "@/lib/editorial-routes";
import { docHref, docPages } from "@/lib/docs";

import { DocNavigation } from "@/components/doc-navigation";
import { CopyButton } from "@/components/copy-button";
import { localizedPath } from "@/lib/locales";
import { useSiteLanguage } from "@/lib/site-language";

const files = import.meta.glob("../../../../docs/**/*.md", { eager: true, query: "?raw", import: "default" }) as Record<string, string>;
const redirects: Record<string, string> = { "/guide/quickstart": "/guide", "/reference/cli": "/reference", "/recover": "/dashboard" };
const isPricingLink = (href?: string) => /^\/pricing\/?(?:[?#]|$)/.test(href ?? "");
function customAnchors() {
  return (tree: Parameters<typeof visit>[0]) => visit(tree, "element", (node) => {
    const element = node as unknown as { tagName: string; properties: Record<string, unknown>; children: Array<{ type: string; value?: string }> };
    if (!/^h[1-6]$/.test(element.tagName)) return;
    const last = element.children.at(-1);
    const anchor = last?.type === "text" && last.value?.match(/\s*\{#([a-z0-9-]+)\}$/i);
    if (!anchor || !last?.value) return;
    last.value = last.value.slice(0, -anchor[0].length);
    element.properties.id = anchor[1];
  });
}
export default function Article() {
  const location = useLocation();
  const { locale, text, href, carryCampaign } = useSiteLanguage();
  const chinese = () => locale() === "zh-TW";
  const article = createMemo(() => docPages.find(({ route }) => route === localizedPath(location.pathname, "en")));
  return <Show when={redirects[location.pathname]} fallback={
    <Show when={article()} keyed fallback={<div class="wrap content-section"><HttpStatusCode code={404}/><PageMeta title="Page not found · Lockdocs" description="This Lockdocs page does not exist." unlisted/><h1>Page not found</h1><A href="/docs">Open the docs</A></div>}>
      {(page) => <div class="wrap docs-layout">
        <PageMeta title={text(page.title, "文件總覽") + " · Lockdocs"} description={text(page.description, "學會安裝 Lockdocs、設定 MCP，並查閱專案實際安裝版本的 API 文件。")}/>
        <DocNavigation />
        <article class="doc-body"><div class="kicker"><span/> {text("LOCKDOCS MANUAL", "LOCKDOCS 使用指南")}</div>
        <Show when={page.route === "/docs"}><h1>{text("Documentation", "文件總覽")}</h1><p>{text("Look up APIs for the version you installed. Start in the terminal, then give the same answers to your coding agent.", "查閱你實際安裝版本的 API。先在終端機試用，再讓程式開發代理工具查閱同一版本的文件。")}</p><div class="doc-start"><A href="/guide"><Terminal size={18}/> {text("Quickstart", "快速入門（英文）")} <ArrowRight size={15}/></A><A href="/reference"><BookOpen size={18}/> {text("CLI reference", "CLI 參考（英文）")} <ArrowRight size={15}/></A><A href={editorGuidePath}>{text("Cursor & Claude Code setup", "設定 Cursor 與 Claude Code（英文）")} <ArrowRight size={15}/></A></div></Show>
        <Show when={chinese()} fallback={
        <Markdown remarkPlugins={[remarkGfm]} rehypePlugins={[customAnchors,rehypeSlug]} transformLinkUri={(url) => {
          const target = docHref(url, page);
          return isPricingLink(target) ? carryCampaign(target) : target;
        }} components={{ pre: (props) => {
          const code = props.node.children.find(child => child.type === "element" && child.tagName === "code");
          const value = code?.type === "element" ? code.children.map(child => child.type === "text" ? child.value : "").join("").replace(/\n$/, "") : "";
          return <div class="doc-code"><div class="doc-code-toolbar"><CopyButton value={value ?? ""} label="Copy code"/></div><pre tabindex="0">{props.children}</pre></div>;
        }, a: page.route === "/pro" ? (props) => <Show when={isPricingLink(props.href) && props.href} fallback={<a href={props.href} title={props.title}>{props.children}</a>}>
          {(target) => <A class="button primary" href={target()}>{props.children}<ArrowRight size={16}/></A>}
        </Show> : "a", text: (props) => <PackageText text={props.node.value}/>, img: (props) => <span><PackageText text={props.alt ?? ""}/></span>, table: (props) => <div class="table-scroll" tabindex="0"><table>{props.children}</table></div> }}>{files["../../../../docs/" + page.source]?.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, "")}</Markdown>
        }>
          <h2>安裝 Lockdocs</h2>
          <p>有 Node.js 的話，在專案根目錄執行下列指令，為編輯器設定 Lockdocs。使用 Python 的話，可改用 <code>uvx lockdocs setup</code>，或用 <code>pip install lockdocs</code> 安裝 CLI。</p>
          <div class="doc-code"><div class="doc-code-toolbar"><CopyButton label="複製指令" value="npx -y @sylphx/lockdocs setup"/></div><pre tabindex="0"><code>npx -y @sylphx/lockdocs setup</code></pre></div>
          <h2>從 lockfile 查閱 API</h2>
          <p>Lockdocs 會讀取專案的 lockfile，根據指定版本查找文件、函式簽名和範例。你可以先在<A href={href("/playground")}>試用工具</A>貼上 lockfile，確認套件版本；若要搜尋完整文件，請在自己的專案使用 CLI 或 MCP。</p>
        </Show>
        <Show when={page.route === "/compare"}><p><A href={context7AlternativePath}>Looking for a Context7 alternative? See a version-sensitive Next.js example.</A></p></Show>
        </article>
      </div>}
    </Show>
  }>{(route) => <Navigate href={route()}/>}</Show>;
}

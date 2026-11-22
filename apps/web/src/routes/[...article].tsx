import { A, Navigate, useLocation } from "@solidjs/router";
import { HttpStatusCode } from "@solidjs/start";
import { createMemo, For, Show } from "solid-js";
import { ArrowRight, BookOpen, Terminal } from "lucide-solid";
import { SolidMarkdown as Markdown } from "solid-markdown";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";
import { visit } from "unist-util-visit";
import { PackageText } from "@/components/package-text";
import { PageMeta } from "@/components/page-meta";
import { context7AlternativePath, editorGuidePath } from "@/lib/editorial-routes";
import { docGroups, docHref, docPages } from "@/lib/docs";

const files = import.meta.glob(["../../../../docs/**/*.md", "!../../../../docs/storefront/**"], { eager: true, query: "?raw", import: "default" }) as Record<string, string>;
const redirects: Record<string, string> = { "/guide/quickstart": "/guide", "/reference/cli": "/reference", "/recover": "/dashboard" };
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
  const article = createMemo(() => docPages.find(({ route }) => route === location.pathname));
  return <Show when={redirects[location.pathname]} fallback={
    <Show when={article()} keyed fallback={<div class="wrap content-section"><HttpStatusCode code={404}/><PageMeta title="Page not found · Lockdocs" description="This Lockdocs page does not exist." unlisted/><h1>Page not found</h1><A href="/docs">Open the docs</A></div>}>
      {(page) => <div class="wrap docs-layout">
        <PageMeta title={page.title + " · Lockdocs"} description={page.description}/>
        <aside class="docs-sidebar"><nav aria-label="Documentation"><A class="docs-brand" href="/docs"><BookOpen size={16}/> Lockdocs docs</A><For each={docGroups}>{group => <div class="docs-group"><div>{group.label}</div><For each={group.pages}>{item => <A href={item.route} end>{item.title}</A>}</For></div>}</For></nav></aside>
        <article class="doc-body"><div class="kicker"><span/> LOCKDOCS MANUAL</div>
        <Show when={page.route === "/docs"}><h1>Documentation</h1><p>Look up APIs for the version you installed. Start in the terminal, then give the same answers to your coding agent.</p><div class="doc-start"><A href="/guide"><Terminal size={18}/> Quickstart <ArrowRight size={15}/></A><A href="/reference"><BookOpen size={18}/> CLI reference <ArrowRight size={15}/></A><A href={editorGuidePath}>Cursor &amp; Claude Code setup <ArrowRight size={15}/></A></div></Show>
        <Markdown remarkPlugins={[remarkGfm]} rehypePlugins={[customAnchors,rehypeSlug]} transformLinkUri={(url) => docHref(url,page)} components={{ text: (props) => <PackageText text={props.node.value}/>, img: (props) => <span><PackageText text={props.alt ?? ""}/></span>, table: (props) => <div class="table-scroll" tabindex="0"><table>{props.children}</table></div> }}>{files["../../../../docs/" + page.source]?.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, "")}</Markdown>
        <Show when={page.route === "/compare"}><p><A href={context7AlternativePath}>Looking for a Context7 alternative? See a version-sensitive Next.js example.</A></p></Show>
        </article>
      </div>}
    </Show>
  }>{(route) => <Navigate href={route()}/>}</Show>;
}

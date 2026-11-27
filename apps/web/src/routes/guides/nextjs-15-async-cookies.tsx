import { JsonLd } from "@/components/json-ld";
import { A } from "@solidjs/router";
import { For } from "solid-js";
import { CodeExample, EditorialHero } from "@/components/editorial";
import { PageMeta } from "@/components/page-meta";
import { PackageText } from "@/components/package-text";
import { editorGuidePath, nextCookiesRecipePath } from "@/lib/editorial-routes";
import { canonicalOrigin } from "@/lib/indexing";
import { lockdocsSoftware } from "@/lib/storefront-schema";
import { upgradePreviewPath, upgrades } from "@/lib/upgrade-examples";
import { useSiteLanguage } from "@/lib/site-language";

// Blocks labelled "Actual output" are copied from runs of the fixture in
// "Reproduce this recipe" (27 November 2026). Change them only with a new run.
const title = "Next.js 15 made cookies() async: fix cookies().get()";
const description = "Next.js 15 changed cookies() to return a Promise. Read the declaration your installed next package ships, then fix cookies().get() by hand or with the codemod.";
const url = canonicalOrigin + nextCookiesRecipePath;
const lockdocs = "npx -y @sylphx/lockdocs@0.7.12";
const lookup = lockdocs + " api cookies --pkg next/headers";
const sylphx = { "@type": "Organization", name: "Sylphx Limited" } as const;
const sections = [
  ["the-change", "The change"],
  ["check-installed", "Ask the installed next"],
  ["fix", "Fix the call site"],
  ["check", "Check it"],
  ["editor", "Let your editor read it"],
  ["before-you-bump", "List call sites before you bump"],
  ["reproduce", "Reproduce this recipe"],
] as const;

const tscError = "app/account/page.tsx(8,27): error TS2339: Property 'get' does not exist on type 'Promise<ReadonlyRequestCookies>'.";
const diff = [
  '@@ -3,9 +3,9 @@ import { redirect } from "next/navigation"',
  " ",
  " // Only signed-in users can open this page.",
  " ",
  "-export default function AccountPage() {",
  "+export default async function AccountPage() {",
  "   // Read the session cookie set at sign-in.",
  '-  const token = cookies().get("session")',
  '+  const token = (await cookies()).get("session")',
  '   if (!token) redirect("/sign-in")',
  "   return <h1>Your account</h1>",
  " }",
].join("\n");
const lookupOutput = (version: string, commit: string, files: number, source: string, signature: string) => [
  `next@${version} · npm · node_modules/next · pinned in package-lock.json`,
  `Upstream: github.com/vercel/next.js@v${version} commit ${commit} (${files} files)`,
  "Public import: `next/headers` · symbol `cookies`",
  "",
  "Declaration entrypoint: headers.d.ts (legacy)",
  "",
  `### cookies (function) — next@${version} ${source}`,
  "```ts",
  signature,
  "```",
].join("\n");
const next14 = { version: "14.2.35", source: "dist/client/components/headers.d.ts:13", signature: "export declare function cookies(): ReadonlyRequestCookies" };
const next15 = { version: "15.5.29", source: "dist/server/request/cookies.d.ts:24", signature: "export declare function cookies(): Promise<ReadonlyRequestCookies>" };
const next14Output = lookupOutput(next14.version, "7b940d9ce96faddb9f92ff40f5e35c34ace04eb2", 317, next14.source, next14.signature);
const next15Output = lookupOutput(next15.version, "17014e9cf7891cc353ebfd8fb4a407f9e4cbbf47", 372, next15.source, next15.signature);
const upgradeCommands = "npm install next@15.5.29 react@19.3.1 react-dom@19.3.1\nnpm install -D @types/react@19.3.3\n" + lookup;
const codemodOutput = "Results: \n0 errors\n0 unmodified\n0 skipped\n1 ok";
const cursorConfig = [
  "{",
  '  "mcpServers": {',
  '    "lockdocs": { "command": "npx", "args": ["-y", "@sylphx/lockdocs", "mcp"] }',
  "  }",
  "}",
].join("\n");
const packageJson = JSON.stringify({
  name: "account-next14", private: true,
  dependencies: { next: "14.2.35", react: "18.3.1", "react-dom": "18.3.1" },
  devDependencies: { "@types/node": "20.17.6", "@types/react": "18.3.12", typescript: "5.6.3" },
}, null, 2);
const pageBefore = [
  'import { cookies } from "next/headers"',
  'import { redirect } from "next/navigation"',
  "",
  "// Only signed-in users can open this page.",
  "",
  "export default function AccountPage() {",
  "  // Read the session cookie set at sign-in.",
  '  const token = cookies().get("session")',
  '  if (!token) redirect("/sign-in")',
  "  return <h1>Your account</h1>",
  "}",
].join("\n");
const tsconfig = [
  "{",
  '  "compilerOptions": {',
  '    "target": "ES2017",',
  '    "lib": ["dom", "dom.iterable", "esnext"],',
  '    "strict": true,',
  '    "noEmit": true,',
  '    "module": "esnext",',
  '    "moduleResolution": "bundler",',
  '    "jsx": "preserve",',
  '    "skipLibCheck": true,',
  '    "isolatedModules": true,',
  '    "esModuleInterop": true',
  "  },",
  '  "include": ["app/**/*.tsx"]',
  "}",
].join("\n");
const reproduceCommands = [
  "cd account-next14",
  "npm install            # writes package-lock.json; Lockdocs reads the pinned version from it",
  "npx tsc --noEmit       # exit code 0 on 14.2.35",
  lookup,
].join("\n");
const syntheticNote = "The account page is a synthetic type-check example, not production authentication. A session cookie being present doesn’t prove who the visitor is.";

export default function NextCookiesRecipe() {
  const { campaignHref, carryCampaign, href } = useSiteLanguage();
  const [previewFrom, previewTo] = [upgrades.next.oldSource, upgrades.next.newSource].map(source => source.split(" ")[0]);
  return <>
    <PageMeta title={title + " · Lockdocs"} description={description}/>
    <JsonLd data={{
      "@context": "https://schema.org",
      "@type": "TechArticle",
      "@id": url + "#recipe",
      headline: title,
      description,
      url,
      inLanguage: "en",
      datePublished: "2026-11-27",
      author: sylphx,
      publisher: sylphx,
      about: lockdocsSoftware,
    }}/>
    <EditorialHero label="LOCKDOCS / RECIPE" title={title} intro="In Next.js 15, cookies() from next/headers returns a Promise. Code that calls cookies().get() type-checks on Next.js 14 and fails on 15. Check what the next package you installed declares, then fix the call site.">
      <aside class="editorial-aside">
        <p class="eyebrow">ON THIS PAGE</p>
        <nav aria-label="Recipe sections">
          <For each={sections}>{([id, name]) => <a href={"#" + id}>{name}</a>}</For>
        </nav>
        <small>Tested on 27 November 2026 with Lockdocs 0.7.12, Node.js 22, npm 10.9 and TypeScript 5.6.3. Blocks labelled “Actual output” are copied from those runs.</small>
      </aside>
    </EditorialHero>
    <div class="wrap"><div class="editorial-body">
      <section id="the-change" class="editorial-section" aria-labelledby="the-change-title">
        <h2 id="the-change-title">The change</h2>
        <p>A Server Component that calls <code>cookies().get("session")</code> type-checks on Next.js 14. On Next.js 15, <code>tsc</code> fails:</p>
        <CodeExample label="Actual output: tsc on Next.js 15.5.29" code={tscError}/>
        <p>This recipe moves one file, <code>app/account/page.tsx</code>, from Next.js 14.2.35 to 15.5.29. It uses Lockdocs to read the <code>cookies()</code> declaration from the <code>next</code> package in <code>node_modules</code>, so neither you nor your coding agent has to rely on memory.</p>
        <CodeExample label="Actual output: git diff after the codemod (step 2)" code={diff}/>
        <p class="editorial-note">{syntheticNote}</p>
        <div class="editorial-choices">
          <For each={[next14, next15]}>{next => <div>
            <h3>Next.js {next.version}</h3>
            <p><code>{next.signature}</code></p>
            <p><code>{next.source}</code></p>
          </div>}</For>
        </div>
        <p class="editorial-note">Next.js documents this in <a href="https://nextjs.org/docs/app/guides/upgrading/version-15#async-request-apis-breaking-change">Async Request APIs (breaking change)</a> and the <a href="https://nextjs.org/docs/app/api-reference/functions/cookies"><code>cookies</code> API reference</a>.</p>
      </section>
      <ol class="editorial-steps" role="list">
        <li id="check-installed" class="editorial-section editorial-step">
          <h2>Ask the installed <code>next</code> what <code>cookies()</code> returns</h2>
          <p>Run this from the folder that contains your lockfile, after <code>npm install</code>:</p>
          <CodeExample label="Look up cookies() in next/headers" code={lookup}/>
          <p><code>--pkg next/headers</code> asks for the import path your code uses. With <code>--pkg next</code>, Lockdocs answers <code>not_exported</code>, because the package root doesn’t export <code>cookies</code>, and lists other import paths, including <code>next/headers</code>.</p>
          <CodeExample label="Actual output: Next.js 14 project" code={next14Output}/>
          <p>Now upgrade. Use the latest patch in the 15.x line, which was 15.5.29 (npm tag <code>backport</code>) when this was written. In our run, <code>npm audit</code> reported a moderate advisory for <code><PackageText text="next@15.5.29"/></code> that is fixed in Next.js 16. Check current Next.js advisories before you choose a version for a deployed application.</p>
          <CodeExample label="Upgrade, then look up cookies() again" code={upgradeCommands}/>
          <p>npm may print <code>ERESOLVE overriding peer dependency</code> while React 18 is being replaced. In our run the install still completed with exit code 0.</p>
          <CodeExample label="Actual output: same project on Next.js 15.5.29" code={next15Output}/>
          <p>The <code>Upstream:</code> line names the public Next.js docs that Lockdocs downloaded from GitHub for that exact release tag on its first query in the project. The first query can also download a 130 MB embedding model once; set <code>LOCKDOCS_EMBED=0</code> to keep search keyword-only. Once the tool and docs are cached, add <code>--offline</code> to skip downloads. The declaration still comes from <code>node_modules</code>.</p>
          <p>To get the guide text for your version as well as the signature:</p>
          <CodeExample label="Ask the Next.js docs for your version" code={lockdocs + ' next "is cookies() async"'}/>
          <p>Actual output, excerpt: on 14.2.35, the top section is the Server Actions example from <code><PackageText text="next@14.2.35 upstream:docs/02-app/01-building-your-application/02-data-fetching/02-server-actions-and-mutations.mdx:854"/></code>, written as <code>cookies().get('name')?.value</code>. On 15.5.29, the answer cites <code><PackageText text="next@15.5.29 upstream:docs/01-app/03-api-reference/03-file-conventions/route.mdx:125"/></code> and <code><PackageText text="next@15.5.29 upstream:docs/01-app/03-api-reference/04-functions/cookies.mdx:1"/></code>.</p>
        </li>
        <li id="fix" class="editorial-section editorial-step">
          <h2>Fix the call site</h2>
          <p><strong>By hand:</strong> make the Server Component <code>async</code> and await <code>cookies()</code>, as in the diff above. TypeScript also accepts the awaited form on Next.js 14.2.35 (<code>tsc</code> exit code 0), so you can merge the fix before you bump the version.</p>
          <p><strong>With the official codemod:</strong> commit your changes first, so the codemod runs in a clean git working tree. Review its diff before you keep it.</p>
          <CodeExample label="Run the Next.js codemod" code="npx @next/codemod@16.4.6 next-async-request-api ."/>
          <CodeExample label="Actual output: @next/codemod 16.4.6" code={codemodOutput}/>
          <p>The diff it produced is the one at the top of this page. If the codemod can’t migrate a call automatically, it adds an <code>@next-codemod-error</code> comment and may add a temporary <code>UnsafeUnwrappedCookies</code> cast. Search for both before you ship.</p>
        </li>
        <li id="check" class="editorial-section editorial-step">
          <h2>Check it</h2>
          <CodeExample label="Type-check the project" code="npx tsc --noEmit"/>
          <CodeExample label="Actual output: Next.js 15.5.29 before the fix (exit code 2)" code={tscError}/>
          <p>After the fix, <code>tsc</code> prints nothing and exits with code 0.</p>
        </li>
        <li id="editor" class="editorial-section editorial-step">
          <h2>Let Cursor or Claude Code read the same declaration</h2>
          <p>Connect Lockdocs to your editor once:</p>
          <CodeExample label="Configure your MCP clients" code={lockdocs + " setup"}/>
          <p><code>setup</code> detects Claude Code, Codex, Cursor, VS Code, Claude Desktop, Windsurf and Gemini CLI, writes their MCP config and prints every change. If you only use Claude Code:</p>
          <CodeExample label="Claude Code" code={"claude mcp add lockdocs -- " + lockdocs + " mcp"}/>
          <p>For Cursor, use the one-click install link in the <a href="https://github.com/SylphxAI/lockdocs#readme">Lockdocs README</a> or add the server to <code>.cursor/mcp.json</code>:</p>
          <CodeExample label=".cursor/mcp.json" code={cursorConfig}/>
          <p>Then ask:</p>
          <CodeExample label="Prompt for your editor" code="Use lockdocs: what does `cookies()` from `next/headers` return in this project? Cite the declaration."/>
          <p>The citation should match step 1: <code><PackageText text={"next@15.5.29 " + next15.source}/></code>. When we called the MCP server’s <code>api</code> tool directly with the same project, it returned that line. If your editor shows a different version, it is reading another folder: pass the absolute path of the folder with your lockfile as <code>root</code>. <A href={campaignHref(editorGuidePath)}>Up-to-date docs for Cursor and Claude Code</A> covers each step.</p>
        </li>
      </ol>
      <section id="before-you-bump" class="editorial-section" aria-labelledby="before-you-bump-title">
        <h2 id="before-you-bump-title">Optional: list every affected call site before you bump</h2>
        <p>From the Next.js 14 project:</p>
        <CodeExample label="Upgrade report" code={lockdocs + " upgrade next 15.5.29 --fetch"}/>
        <p><code>--fetch</code> downloads exactly <code><PackageText text="next@15.5.29"/></code> from the npm registry so Lockdocs can compare it with the installed version. Without it, Lockdocs won’t compare against the version in <code>node_modules</code>; it asks you to add <code>--fetch</code>. The full report lists each changed API, its replacement and your call sites. It is part of <A href={campaignHref("/pro")}>Lockdocs Pro</A>. Without a licence, the command prints a one-line summary with one sample call site and exits with code 3.</p>
        <p class="editorial-note">Illustrative: the Playground has a preview of the report for this change. It is a bundled example (<code><PackageText text={previewFrom + " → " + previewTo}/></code>, <code>{upgrades.next.path}</code>), not a scan of your code. <A href={carryCampaign(upgradePreviewPath(href("/playground"), "next"))}>See the Next.js 14 → 15 report preview</A></p>
      </section>
      <section id="reproduce" class="editorial-section" aria-labelledby="reproduce-title">
        <h2 id="reproduce-title">Reproduce this recipe</h2>
        <CodeExample label="account-next14/package.json" code={packageJson}/>
        <p><code>account-next14/app/account/page.tsx</code> is the “before” file; the <code>cookies()</code> call is on line 8. {syntheticNote}</p>
        <CodeExample label="account-next14/app/account/page.tsx" code={pageBefore}/>
        <CodeExample label="account-next14/tsconfig.json" code={tsconfig}/>
        <CodeExample label="Install and check on Next.js 14" code={reproduceCommands}/>
        <p>Continue from the upgrade commands in step 1. Neither install needs <code>--force</code> or <code>--legacy-peer-deps</code>.</p>
      </section>
    </div></div>
  </>;
}

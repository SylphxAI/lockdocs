import { JsonLd } from "@/components/json-ld";
import { A } from "@solidjs/router";
import { For } from "solid-js";
import { ArrowRight } from "lucide-solid";
import { useCookieSettings } from "@/components/cookie-settings";
import { EditorMcpTabs } from "@/components/editor-mcp-tabs";
import { CodeExample, EditorialFaq, EditorialHero } from "@/components/editorial";
import { PageMeta } from "@/components/page-meta";
import { PackageText } from "@/components/package-text";
import { context7AlternativePath, editorGuidePath } from "@/lib/editorial-routes";
import { canonicalOrigin } from "@/lib/indexing";
import { pypiUrl } from "@/lib/install";
import { useSiteLanguage } from "@/lib/site-language";
import { upgradePreviewPath } from "@/lib/upgrade-examples";

const title = "Up-to-date docs for Cursor and Claude Code";
const description = "Connect Lockdocs to Cursor or Claude Code over local MCP, then check that the package version and API citation match your project’s lockfile.";
const url = canonicalOrigin + editorGuidePath;
const steps = [
  {
    id: "verify-cli", name: "Check the CLI in your project",
    text: "Run the commands from the folder with your lockfile. Confirm the CLI prints its own version, then find the library you want in the resolve output. If you’re trying this in a throwaway npm project, install Zod first to follow the example below.",
  },
  {
    id: "connect", name: "Connect Cursor or Claude Code",
    text: "Pick your editor below. Both configurations launch the same local MCP server over standard input and output. Keep the project open in your editor.",
  },
  {
    id: "verify-citation", name: "Match the editor’s answer to the CLI",
    text: "Ask your editor to call Lockdocs. Give it the absolute path to the folder with your lockfile. Check that the version from resolve and the declaration citation from api match what you see in the terminal.",
  },
] as const;
const faq = [
  {
    question: "Does up-to-date mean the newest release?",
    answer: "It means the version in this project’s lockfile. After a dependency upgrade, update the lockfile and install the new version before running resolve again.",
  },
  {
    question: "Why does resolve show a different project or no dependencies?",
    answer: "Use the absolute path of the folder with the lockfile as the root tool argument. In a monorepo, check that you are testing the intended package. If a local dependency is missing or out of sync, install from the lockfile before checking the declaration.",
  },
  {
    question: "Can I use the server without internet access?",
    answer: "Install the CLI and dependencies while online. For a fully offline launch, use the absolute path to an already installed lockdocs executable as the MCP command and pass mcp --offline as its arguments. Local and cached files remain available; missing upstream docs will not download.",
  },
];

export default function EditorDocsGuide() {
  const { campaignHref, carryCampaign, href, proHref } = useSiteLanguage();
  const zodExample = () => carryCampaign(upgradePreviewPath(href("/playground"), "zod"));
  const { trackEvent } = useCookieSettings();
  return <>
    <PageMeta title={title + " · Lockdocs"} description={description}/>
    <JsonLd data={{
      "@context": "https://schema.org",
      "@type": "HowTo",
      "@id": url + "#setup",
      name: title,
      description,
      step: steps.map(({ id, name, text }) => ({ "@type": "HowToStep", name, text, url: url + "#" + id })),
    }}/>
    <EditorialHero label="LOCKDOCS / SETUP GUIDE" title={title} intro="Give your editor the docs for the version you run. Connect Lockdocs over MCP, then check one citation against your lockfile before relying on its answers." actions={<>
      <A class="button primary" href={zodExample()}>Try the Zod example <ArrowRight size={16} aria-hidden="true"/></A>
      <A class="button outline" href={campaignHref(context7AlternativePath)}>Compare with Context7</A>
    </>}>
      <aside class="editorial-aside">
        <p class="eyebrow">ON THIS PAGE</p>
        <nav aria-label="Guide sections">
          <For each={steps}>{step => <a href={"#" + step.id}>{step.name}</a>}</For>
        </nav>
        <small>The npm examples use Node.js 18+. Lockdocs runs on macOS (Arm64 or x64), Linux with glibc (Arm64 or x64), and Windows x64. No Lockdocs account or API key needed.</small>
      </aside>
    </EditorialHero>
    <div class="wrap"><div class="editorial-body">
      <ol id="setup" class="editorial-steps" role="list">
        <li id={steps[0].id} class="editorial-section editorial-step">
          <h2>{steps[0].name}</h2>
          <p>{steps[0].text}</p>
          <p>Using Python? Run the <a href={pypiUrl}>PyPI</a> release with <code>uvx lockdocs</code>, or run <code>pip install lockdocs</code> and use <code>lockdocs</code> directly. To connect your editor without npm, see <A href={campaignHref("/guide/setup#without-nodejs")}>MCP setup without Node.js</A>.</p>
          <CodeExample label="Optional scratch project" code={"mkdir lockdocs-editor-demo\ncd lockdocs-editor-demo\nnpm init -y\nnpm install --save-exact zod@4.6.7"}/>
          <CodeExample label="CLI version and lockfile check" code={"npx -y @sylphx/lockdocs version\nnpx -y @sylphx/lockdocs resolve --offline"}/>
          <p>In the scratch project, look for <code><PackageText text="zod@4.6.7"/></code> and <code>package-lock.json</code>. The first <code>npx</code> run may download Lockdocs; <code>--offline</code> applies to Lockdocs’s lookups, not to npm.</p>
          <p>Want to see what changes between versions? <A href={zodExample()}>Try the Zod example in the playground</A> to see Zod 3.23.8 resolve from a sample <code>yarn.lock</code> and compare its API with Zod v4.</p>
        </li>
        <li id={steps[1].id} class="editorial-section editorial-step">
          <h2>{steps[1].name}</h2>
          <p>{steps[1].text}</p>
          <EditorMcpTabs/>
        </li>
        <li id={steps[2].id} class="editorial-section editorial-step">
          <h2>{steps[2].name}</h2>
          <p>{steps[2].text}</p>
          <CodeExample label="Editor verification prompt" code={"Use Lockdocs resolve with root <absolute path to my project>.\nThen call Lockdocs api with the same root, package zod, and symbol z.strictObject.\nShow the package version, file and line from the result."}/>
          <CodeExample label="Terminal cross-check" code="npx -y @sylphx/lockdocs api z.strictObject --pkg zod --offline"/>
          <p>Replace the placeholder in the prompt. In your own project, substitute an installed library and a known symbol if you don’t use Zod. If the versions differ, check the tool’s <code>root</code> and reinstall from the lockfile. If the server doesn’t start, check that the editor can run <code>npx</code> and has the same Node.js setup as your terminal.</p>
        </li>
      </ol>
      <EditorialFaq items={faq} url={url}/>
      <section class="editorial-section" aria-labelledby="next-title">
        <h2 id="next-title">Look up your next API</h2>
        <p>Use <code>api</code> for a symbol you know and <code>docs</code> for a question about a package.</p>
        <div class="button-row">
          <A class="button primary" href={campaignHref("/reference/tools")}>Read the MCP tool reference <ArrowRight size={16}/></A>
          <A class="button outline" href={campaignHref(context7AlternativePath)}>Compare Lockdocs with Context7</A>
          <A class="button outline" href={proHref("guide_cursor_claude_code")} onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "guide_cursor_claude_code" }, { requireExplicitConsent: true })}>See Lockdocs Pro</A>
        </div>
      </section>
    </div></div>
  </>;
}

import { JsonLd } from "@/components/json-ld";
import { A } from "@solidjs/router";
import { ArrowRight } from "lucide-solid";
import { useCookieSettings } from "@/components/cookie-settings";
import { CodeExample, EditorialFaq, EditorialHero } from "@/components/editorial";
import { PageMeta } from "@/components/page-meta";
import { VersionProofAside } from "@/components/version-proof-aside";
import { context7AlternativePath, editorGuidePath } from "@/lib/editorial-routes";
import { canonicalOrigin } from "@/lib/indexing";
import { useSiteLanguage } from "@/lib/site-language";

const title = "Context7 alternative for exact-version docs";
const description = "Compare Context7’s hosted docs search with Lockdocs’s local, lockfile-pinned lookup. Check the API your project actually installed, from the CLI or MCP.";
const url = canonicalOrigin + context7AlternativePath;
const faq = [
  {
    question: "Can I run Lockdocs and Context7 in the same editor?",
    answer: "Yes. Use Lockdocs to check a declaration for your installed version. Use Context7 when you need an indexed guide or an example for a library you have not installed.",
  },
  {
    question: "Does Lockdocs work offline?",
    answer: "Yes, once the binary and the files you need are on disk. Use --offline to prevent Lockdocs downloads. Installing the binary or fetching docs that are not cached still requires a connection.",
  },
  {
    question: "Does Lockdocs prevent hallucinations?",
    answer: "Lockdocs retrieves real documentation and declarations with file and line citations instead of generating API signatures. Search can miss the right passage, and your coding agent can still misread a result. Check the citation before changing code.",
  },
];

export default function Context7Alternative() {
  const { campaignHref, proHref } = useSiteLanguage();
  const { trackEvent } = useCookieSettings();
  return <>
    <PageMeta title={title + " · Lockdocs"} description={description}/>
    <JsonLd data={{
      "@context": "https://schema.org",
      "@type": "SoftwareApplication",
      "@id": canonicalOrigin + "/#software",
      name: "Lockdocs",
      url: canonicalOrigin,
      description: "Look up library documentation and API declarations for the versions in your lockfile.",
      applicationCategory: "DeveloperApplication",
      operatingSystem: "macOS, Linux, Windows",
      downloadUrl: "https://www.npmjs.com/package/@sylphx/lockdocs",
      offers: { "@type": "Offer", price: 0, priceCurrency: "USD", description: "Free local CLI and MCP lookups" },
    }}/>
    <EditorialHero label="LOCKDOCS / COMPARISON" title={title} intro="Use Lockdocs when the answer depends on the library version in your lockfile. Use Context7 when you need a guide or example from a hosted docs index." actions={<>
      <A class="button primary" href={campaignHref("/playground")}>Try the playground <ArrowRight size={16}/></A>
      <A class="button outline" href={proHref("context7_alternative")} onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "context7_alternative" })}>See Lockdocs Pro</A>
    </>}>
      <VersionProofAside/>
    </EditorialHero>
    <div class="wrap"><div class="editorial-body">
      <section class="editorial-section" aria-labelledby="lookup-title">
        <h2 id="lookup-title">Check your project before adding <code>await</code></h2>
        <p>Run this from an npm project with Next.js already installed. The first command shows the version and lockfile. The second looks up <code>cookies</code> through the public import <code>next/headers</code>. Check the package version and the cited <code>.d.ts</code> line before using the result.</p>
        <CodeExample label="Next.js lookup" code={"npx -y @sylphx/lockdocs resolve --offline\nnpx -y @sylphx/lockdocs api cookies --pkg next/headers --offline"}/>
        <p>In MCP, call <code>resolve</code> with your project’s absolute path as <code>root</code>. For the same declaration, call <code>api</code> with that <code>root</code>, <code>package: "next/headers"</code> and <code>symbol: "cookies"</code>. Use <code>docs</code> for questions where you don’t know the symbol yet.</p>
        <p class="editorial-note">If the installed version and lockfile disagree, Lockdocs can report the mismatch and show the installed declaration. Install from the lockfile and check again before treating the declaration as pinned.</p>
      </section>
      <section class="editorial-section" aria-labelledby="sources-title">
        <h2 id="sources-title">Where each tool looks</h2>
        <div class="editorial-choices">
          <div>
            <h3>Lockdocs: the project’s files</h3>
            <p>The lockfile selects the version. Lockdocs reads installed READMEs, docs and declarations, and can fetch upstream docs into a local cache. <code>api</code> matches symbols; <code>docs</code> ranks passages and cites the package, file and line.</p>
            <p>Local lookups have no hosted query quota or API key. A package may ship declarations and very little prose.</p>
          </div>
          <div>
            <h3>Context7: a hosted index</h3>
            <p><code>resolve-library-id</code> searches for a library, and <code>query-docs</code> retrieves relevant snippets. You can specify a version. Its <a href="https://github.com/upstash/context7/blob/5b623bfd383b3c0bb5479738caf74764e5280d76/README.md">documented setup</a> doesn’t select one from your local lockfile.</p>
            <p>A hosted index can be useful for website guides and libraries you haven’t installed. Running the MCP client locally still requires access to the hosted service.</p>
          </div>
        </div>
        <p class="editorial-note"><A href={campaignHref("/guide/how-it-works")}>How Lockdocs searches</A> · <A href={campaignHref("/compare")}>Full comparison</A></p>
      </section>
      <section class="editorial-section" aria-labelledby="bench-title">
        <h2 id="bench-title">The docs you have on disk matter</h2>
        <p>In the repo’s 105-question benchmark of Lockdocs 0.4.0, Lockdocs answered 70 correctly from an empty cache with default fetching. After <code>lockdocs fetch</code>, it answered 96 correctly. The anonymous Context7 arm answered 77 correctly and reused some unchanged answers from earlier runs.</p>
        <p>Those are historical scores under different cache and network conditions. Read the <A href={campaignHref("/benchmarks")}>benchmark setup and misses</A> before applying them to your project.</p>
      </section>
      <EditorialFaq items={faq} url={url}/>
      <section class="editorial-section" aria-labelledby="try-title">
        <h2 id="try-title">Try a version-sensitive lookup</h2>
        <p>Start with an example in the playground, or connect Lockdocs to the project you already have open.</p>
        <div class="button-row">
          <A class="button primary" href={campaignHref("/playground")}>Open playground <ArrowRight size={16}/></A>
          <A class="button outline" href={campaignHref("/docs")}>Read the docs</A>
          <A class="button outline" href={campaignHref(editorGuidePath)}>Set up Cursor or Claude Code</A>
          <A class="button outline" href={proHref("context7_alternative")} onClick={() => trackEvent("pro_cta_clicked", { entry_surface: "context7_alternative" }, { requireExplicitConsent: true })}>See Lockdocs Pro</A>
        </div>
      </section>
    </div></div>
  </>;
}

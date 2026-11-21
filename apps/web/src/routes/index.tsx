import { PageMeta } from "@/components/page-meta";
import { A as Link } from "@solidjs/router";
import { ArrowRight, ArrowUpRight, Box, Command, FileCode2, GitCompareArrows, ShieldCheck, Terminal } from 'lucide-solid';
import { CopyButton } from '@/components/copy-button';

const install = 'npx -y @sylphx/lockdocs setup';
export default function Home() {
  return <>
    <PageMeta title="Lockdocs — Docs for the version you installed" description="Exact-version library docs from your lockfile. Local, offline and open source. For your terminal and your coding agent."/>
    <section class="hero wrap">
      <div class="hero-copy">
        <div class="eyebrow"><span class="signal"/> LOCAL-FIRST DOCUMENTATION</div>
        <h1>Docs for the version <span>you installed.</span></h1>
        <p>Your agent shouldn’t guess which API exists. Lockdocs reads your lockfile and answers from the exact version installed in your project.</p>
        <div class="hero-actions"><Link class="button primary" href="/playground">Try the playground <ArrowRight size={17}/></Link><Link class="button quiet" href="/guide">Read the docs <ArrowUpRight size={15}/></Link></div>
        <div class="install"><Terminal size={16}/><code>{install}</code><CopyButton value={install} label="Copy install command"/></div>
      </div>
      <div class="hero-preview">
        <div class="preview-chrome"><span><FileCode2 size={15}/> VERSION INSPECTOR</span><span class="preview-live"><span class="signal"/> SAMPLE PROJECT</span></div>
        <div class="preview-files"><span><span class="mint">{ '{ }' }</span> package-lock.json</span><span>next@14.2.3</span></div>
        <div class="preview-body"><div class="code-line"><i>12</i><span><b>"node_modules/next"</b>: {'{'}</span></div><div class="code-line focus-line"><i>13</i><span>&nbsp; "version": <em>"14.2.3"</em>,</span></div><div class="code-line"><i>14</i><span>{'}'}</span></div></div>
        <div class="preview-result">
          <div class="preview-label"><Command size={14}/> API LOOKUP <span>next/headers</span></div>
          <div class="diff-line before"><i>−</i><code>cookies(): ReadonlyRequestCookies</code></div>
          <div class="diff-line after"><i>+</i><code>cookies(): Promise&lt;ReadonlyRequestCookies&gt;</code></div>
          <Link href="/playground#upgrade" class="preview-link">Inspect the Next.js 15 upgrade <ArrowUpRight size={14}/></Link>
        </div>
      </div>
    </section>
    <section class="wrap" aria-label="Benchmark highlights">
      <div class="bench-rail">
        <div class="bench-label"><span class="eyebrow">01 / BENCHMARK</span><Link href="/benchmarks">Read the methodology <ArrowUpRight size={14}/></Link></div>
        <div><strong>96<span>/105</span></strong><small>Lockdocs · after prefetch</small></div>
        <div><strong>77<span>/105</span></strong><small>Context7 · anonymous API</small></div>
        <div><strong>58<span>ms</span></strong><small>Lockdocs · median query</small></div>
      </div>
    </section>
    <section class="wrap section how">
      <div class="section-intro"><div class="eyebrow">02 / HOW IT WORKS</div><h2>From lockfile to cited answer.</h2><p>One Rust binary. Three MCP tools. No account needed for the open-source core.</p></div>
      <div class="steps">
        <article><Box size={20}/><h3>Resolve the pin</h3><p>Identify the dependency from your project’s lockfile, not a search for the newest release.</p></article>
        <article><FileCode2 size={20}/><h3>Read its docs</h3><p>Look up guides, signatures and symbols in versioned package files. Every answer points back to its source.</p></article>
        <article><ShieldCheck size={20}/><h3>Keep it local</h3><p>Query your local index from your CLI or coding agent. Fetch upstream docs when you want them.</p></article>
      </div>
    </section>
    <section class="wrap section"><div class="pro-band"><GitCompareArrows size={25}/><div><div class="eyebrow">LOCKDOCS PRO</div><h2>See what breaks before you upgrade.</h2><p>Get the full upgrade report for your call sites and exact-version docs for private packages.</p></div><Link class="button outline" href="/pricing">Explore Pro <ArrowRight size={16}/></Link></div></section>
  </>;
}

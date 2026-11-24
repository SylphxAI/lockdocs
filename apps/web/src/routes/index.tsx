import { EditorialFaq } from "@/components/editorial";
import { JsonLd } from "@/components/json-ld";
import { canonicalOrigin } from "@/lib/indexing";
import { useSiteLanguage } from "@/lib/site-language";
import { lockdocsApplication, freeOffer } from "@/lib/storefront-schema";
import { PageMeta } from "@/components/page-meta";
import { A as Link } from "@solidjs/router";
import { ArrowRight, ArrowUpRight, Box, Command, FileCode2, GitCompareArrows, ShieldCheck } from 'lucide-solid';
import { PackageText } from '@/components/package-text';
import { InstallCommand } from "@/components/install-command";

export default function Home() {
  const { href, campaignHref, text, locale } = useSiteLanguage();
  const url = () => canonicalOrigin + href("/");
  const questions = () => [
    { question: text("Is Lockdocs free?", "Lockdocs 可以免費使用嗎？"), answer: text("Yes. Exact-version docs and the local MCP server are free and don’t require an account. Lockdocs Pro adds Upgrade Reports and Private Package Docs.", "可以。查閱指定版本的文件和使用本機 MCP 伺服器都免費，也不需要帳戶。Lockdocs Pro 另外提供升級報告和私有套件文件。") },
  { question: text("Can I use Lockdocs offline?", "可以離線使用 Lockdocs 嗎？"), answer: text("Yes. Lockdocs can query packages and docs that are already installed or cached. If you want to fetch additional upstream docs, that step needs a network connection.", "可以。已安裝或快取的套件與文件都能離線查詢；下載更多上游文件時才需要連線。") },
  ];

  return <>
    <PageMeta title={text("Lockdocs — Docs for the version you installed", "Lockdocs — 查閱已安裝版本的文件")} description={text("Exact-version library docs from your lockfile. Local, offline and open source. For your terminal and coding agent. Install from npm or PyPI.", "直接從 lockfile 查閱套件指定版本的文件。在本機和離線使用，支援終端機與程式開發代理工具。開放原始碼，可從 npm 或 PyPI 安裝。")}/>
    <JsonLd data={lockdocsApplication(url(), [freeOffer("USD")])}/>
    <section class="hero wrap">
      <div class="hero-copy">
        <div class="eyebrow"><span class="signal"/> {text("LOCAL-FIRST DOCUMENTATION", "在本機查閱文件")}</div>
        <h1>{text("Docs for the version", "查閱你實際安裝")} <span>{text("you installed.", "版本的文件")}</span></h1>
        <p>{text("Your agent shouldn’t guess which API exists. Lockdocs reads your lockfile and answers from the exact version installed in your project.", "不必讓程式開發代理工具猜測有哪些 API。Lockdocs 讀取專案的 lockfile，根據你安裝的確切版本回答問題。")}</p>
        <div class="hero-actions"><Link class="button primary" href={campaignHref("/playground")}>{text("Try the playground", "試用 lockfile 工具")} <ArrowRight size={17}/></Link><Link class="button quiet" href={locale() === "en" ? "/guide" : href("/docs")}>{text("Read the docs", "閱讀文件")} <ArrowUpRight size={15}/></Link></div>
        <InstallCommand />
      </div>
      <div class="hero-preview">
        <div class="preview-chrome"><span><FileCode2 size={15}/> {text("VERSION INSPECTOR", "版本檢視器")}</span><span class="preview-live"><span class="signal"/> {text("SAMPLE PROJECT", "範例專案")}</span></div>
        <div class="preview-files"><span><span class="mint">{ '{ }' }</span> package-lock.json</span><span><PackageText text="next@14.2.3"/></span></div>
        <div class="preview-body"><div class="code-line"><i>12</i><span><b>"node_modules/next"</b>: {'{'}</span></div><div class="code-line focus-line"><i>13</i><span>&nbsp; "version": <em>"14.2.3"</em>,</span></div><div class="code-line"><i>14</i><span>{'}'}</span></div></div>
        <div class="preview-result">
          <div class="preview-label"><Command size={14}/> {text("API LOOKUP", "查閱 API")} <span>next/headers</span></div>
          <div class="diff-line before"><i>−</i><code>cookies(): ReadonlyRequestCookies</code></div>
          <div class="diff-line after"><i>+</i><code>cookies(): Promise&lt;ReadonlyRequestCookies&gt;</code></div>
          <Link href={campaignHref("/playground") + "#upgrade"} class="preview-link">{text("Inspect the Next.js 15 upgrade", "查看升級至 Next.js 15 的變更")} <ArrowUpRight size={14}/></Link>
        </div>
      </div>
    </section>
    <section class="wrap" aria-label={text("Benchmark highlights", "評測摘要")}>
      <div class="bench-rail">
        <div class="bench-label"><span class="eyebrow">{text("01 / BENCHMARK", "01 / 評測結果")}</span><Link href="/benchmarks">{text("Read the methodology", "查看評測方法")} <ArrowUpRight size={14}/></Link></div>
        <div class="bench-lockdocs">
          <h3>{text("With Lockdocs", "使用 Lockdocs")}</h3>
          <div class="bench-score"><strong>96<span>/105</span></strong><span class="bench-delta" aria-label={text("19 more correct answers", "多答對 19 題")}>+19</span></div>
          <small>{text("Correct answers · after prefetch", "答對題數 · 預先下載文件後")}</small>
        </div>
        <div class="bench-baseline">
          <h3>{text("Baseline (without Lockdocs)", "基準（未使用 Lockdocs）")}</h3>
          <strong>77<span>/105</span></strong>
          <small>{text("Correct answers · Context7 anonymous API", "答對題數 · Context7 匿名 API")}</small>
        </div>
        <div class="bench-latency"><h3>{text("Lockdocs query time", "Lockdocs 查詢時間")}</h3><strong>58<span>ms</span></strong><small>{text("Median after prefetch", "預先下載文件後的中位數")}</small></div>
      </div>
    </section>
    <section class="wrap section how">
      <div class="section-intro"><div class="eyebrow">{text("02 / HOW IT WORKS", "02 / 運作方式")}</div><h2>{text("From lockfile to cited answer.", "從 lockfile 找到答案和出處")}</h2><p>{text("One Rust binary. Three MCP tools. No account needed for the open-source core.", "一個 Rust 執行檔，三個 MCP 工具。使用開源功能不必註冊。")}</p></div>
      <div class="steps">
        <article><Box size={20}/><h3>{text("Resolve the pin", "確認套件版本")}</h3><p>{text("Identify the dependency from your project’s lockfile, not a search for the newest release.", "直接讀取專案的 lockfile，找出你實際安裝的版本。")}</p></article>
        <article><FileCode2 size={20}/><h3>{text("Read its docs", "查閱該版本的文件")}</h3><p>{text("Look up guides, signatures and symbols in versioned package files. Every answer points back to its source.", "從指定版本的套件檔案中查閱指南、函式簽名和符號。每則答案都附上出處。")}</p></article>
        <article><ShieldCheck size={20}/><h3>{text("Keep it local", "在本機查詢")}</h3><p>{text("Query your local index from your CLI or coding agent. Fetch upstream docs when you want them.", "用命令列或程式開發代理工具查詢本機索引，需要時再下載上游文件。")}</p></article>
      </div>
    </section>
    <section class="wrap section"><div class="pro-band"><GitCompareArrows size={25}/><div><div class="eyebrow">LOCKDOCS PRO</div><h2>{text("See what breaks before you upgrade.", "升級前先找出會受影響的程式碼")}</h2><p>{text("Get the full upgrade report for your call sites and exact-version docs for private packages.", "取得包含呼叫位置的完整升級報告，也能查閱指定版本的私有套件文件。")}</p></div><Link class="button outline" href={campaignHref("/pricing")}>{text("Explore Pro", "了解 Pro")} <ArrowRight size={16}/></Link></div></section>
    <div class="wrap"><EditorialFaq url={url()} items={questions()} heading={text("Frequently asked questions", "常見問題")}/></div>
  </>;
}

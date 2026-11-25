import type { ParentProps } from "solid-js";
import { createEffect, Suspense } from "solid-js";
import { Router } from "@solidjs/router";
import { FileRoutes } from "@solidjs/start/router";
import { MetaProvider } from "@solidjs/meta";
import { A as Link, useLocation, useSearchParams } from "@solidjs/router";
import { ArrowUpRight, Braces, GitFork } from 'lucide-solid';
import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import './globals.css';
import { context7AlternativePath, editorGuidePath } from "@/lib/editorial-routes";
import { pypiUrl } from "@/lib/install";
import { CookieSettings, CookieSettingsButton } from "@/components/cookie-settings";


import { useSiteLanguage } from "@/lib/site-language";
import { localizedPath, pagePath } from "@/lib/locales";
import { pricingPath } from "@/lib/auth-entry";

const github = 'https://github.com/SylphxAI/lockdocs';
function Wordmark() {
  const { campaignHref } = useSiteLanguage();
  return <Link href={campaignHref("/")} class="wordmark"><img src="/brand/lockdocs.svg" alt="" width="31" height="31"/><span>lockdocs<span class="wordmark-dot">.</span></span></Link>;
}
function Layout(props: ParentProps) {
  const { locale, campaignHref, carryCampaign, text } = useSiteLanguage();
  const location = useLocation();
  const [query] = useSearchParams();
  createEffect(() => { document.documentElement.lang = locale(); });
  const targetLocale = () => locale() === "en" ? "zh-TW" : "en";
  const switchHref = () => {
    const current = pagePath(location.pathname);
    const path = localizedPath(current, targetLocale());
    if (path === current) return carryCampaign(localizedPath("/", targetLocale()));
    return carryCampaign(localizedPath(path, "en") === "/pricing"
      ? pricingPath(typeof query.currency === "string" ? query.currency : "USD", query.entry_surface, targetLocale())
      : path);
  };
  return <>
    <a class="skip" href="#main">{text("Skip to content", "跳至主要內容")}</a>
    <header class="site-header"><div class="wrap header-inner">
      <Wordmark />
      <nav aria-label={text("Main navigation", "主選單")} class="main-nav"><Link href={campaignHref("/docs")}>{text("Docs", "文件")}</Link><Link href={campaignHref("/playground")}>{text("Playground", "試用工具")}</Link><Link href={campaignHref("/pricing")}>{text("Pricing", "價格")}</Link></nav>
      <div class="header-tools"><Link class="language-link" href={switchHref()} lang={targetLocale()} hreflang={targetLocale()}>{text("繁體中文", "English")}</Link><a class="repo-link" href={github} aria-label="Lockdocs on GitHub"><GitFork size={17}/></a><Link href="/dashboard" class="account-link">{text("Account", "帳戶")} <ArrowUpRight size={13}/></Link></div>
    </div></header>
    <main id="main"><Suspense>{props.children}</Suspense></main>
    <footer class="site-footer"><div class="wrap footer-inner"><Wordmark/><p>{text("Exact-version docs, by Sylphx.", "由 Sylphx 打造，查閱指定版本的文件。")} </p><nav aria-label={text("Footer navigation", "頁尾導覽")}><Link href="/benchmarks">{text("Benchmarks", "評測結果")}</Link><Link href="/compare">{text("Compare", "工具比較")}</Link><Link href={context7AlternativePath}>{text("Context7 alternative", "Context7 替代方案")}</Link><Link href={editorGuidePath}>{text("Editor setup", "編輯器設定")}</Link><a href={github}><Braces size={14}/> {text("Source", "原始碼")}</a><a href={pypiUrl}>PyPI</a><Link href="/legal/privacy">{text("Privacy", "隱私權")}</Link><Link href="/legal/terms">{text("Terms", "服務條款")}</Link><CookieSettingsButton /></nav></div></footer>
  </>;
}

export default function App() {
 return <Router root={props => <MetaProvider><CookieSettings><Layout>{props.children}</Layout></CookieSettings></MetaProvider>}><FileRoutes /></Router>;
}

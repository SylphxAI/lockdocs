import { EditorialFaq } from "@/components/editorial";
import { PricingDecisionProof, PricingProof } from "@/components/pricing-proof";
import { pricingFaq } from "@/lib/pricing-faq";
import { JsonLd } from "@/components/json-ld";
import { canonicalOrigin } from "@/lib/indexing";
import { useSiteLanguage } from "@/lib/site-language";
import { lockdocsApplication, freeOffer, proOffer } from "@/lib/storefront-schema";
import { createMemo, createSignal, For, onMount, Suspense } from "solid-js";
import { createAsync, useSearchParams } from "@solidjs/router";
import { account } from "@/lib/account-query";
import { PageMeta } from "@/components/page-meta";
import { A as Link } from "@solidjs/router";
import { ArrowRight, Check, LockKeyhole } from "lucide-solid";
import { billingCurrencies, knownCheckoutSurface, pricingCheckoutSurface } from "@/lib/analytics-events";
import { pricingPath, signInHref } from "@/lib/auth-entry";
import { useCookieSettings } from "@/components/cookie-settings";
import { attemptIntent, checkoutIntentField, continuesAttempt, type CheckoutAttempt } from "@/lib/checkout-intent";
import { annualAmount, annualBilling, perYear, pricePrefixes as prefixes } from "@/lib/pro-plan";

const currencies = billingCurrencies;

export default function Pricing() {
  const { href, carryCampaign, text, locale } = useSiteLanguage();
  const pricingUrl = () => canonicalOrigin + href("/pricing");
  const [query] = useSearchParams();
  const analytics = useCookieSettings();
  const currency = () => currencies.find((code) => code === query.currency) ?? "USD";
  const entrySurface = () => knownCheckoutSurface(query.entry_surface) ?? pricingCheckoutSurface;
  const currencyHref = (code: string) => pricingPath(code, entrySurface(), locale());
  // SSR and the first client render agree. After hydration, follow the browser's locale.
  const [locales, setLocales] = createSignal<readonly string[]>(["en"]);
  onMount(() => setLocales(navigator.languages));
  const numberLocale = () => locale() === "en" ? locales() : locale();
  const whole = createMemo(() => new Intl.NumberFormat(numberLocale()));
  const money = (value: number, code = currency()) => prefixes[code] + whole().format(value);
  const annual = () => money(annualAmount(currency()));
  const returnPath = () => currencyHref(currency());
  const details = createAsync(() => account(), { deferStream: true });
  const session = () => details()?.user;
  const entitlement = () => details()?.entitlement;
  const checkoutPage: { attempt?: CheckoutAttempt } = {};
  return <>
    <PageMeta title={text("Pricing · Lockdocs", "價格 · Lockdocs")} description={text("Compare Free and Lockdocs Pro, with tax-inclusive annual pricing in four currencies.", "比較免費版與 Lockdocs Pro。年費已含稅，支援四種付款幣別。")}/>
    <JsonLd data={lockdocsApplication(pricingUrl(), [freeOffer(currency()), proOffer(currency(), canonicalOrigin + pricingPath(currency(), undefined, locale()))])}/>
    <Suspense><div class="wrap content-section pricing-section">
    <div class="page-intro pricing-intro"><div class="kicker"><span/> {text("LOCKDOCS PLANS", "LOCKDOCS 方案")}</div><h1>{text("Know what breaks before you upgrade.", "升級前，先找出受影響的程式碼")}</h1><p>{text("Exact-version docs are free. Pro adds call-site upgrade reports and private package docs.", "免費查閱指定版本的文件。Pro 提供附有呼叫位置的升級報告，以及私有套件文件。")}</p></div>
    <nav class="segmented currency-picker" aria-label={text("Billing currency", "付款幣別")}><For each={currencies}>{(code) => <a aria-current={code === currency() ? "page" : undefined} href={carryCampaign(currencyHref(code))}>{code}</a>}</For></nav>
    <div class="plan-grid">
      <article class="surface plan plan-pro"><div class="plan-label">{text("PAID PLAN", "付費方案")}</div><h2>Lockdocs Pro</h2><div class="plan-price">{annual()}{text(...perYear)}</div><p class="plan-total">{text(...annualBilling)}</p><ul class="check-list"><For each={[text("Everything in Free", "包含免費版的所有功能"), text("Upgrade Reports with call-site citations", "升級報告，附上受影響的呼叫位置"), text("Private Package Docs", "私有套件文件")]}>{(item) => <li><Check size={17}/>{item}</li>}</For></ul>
      {query.billing === "unavailable" && <p role="status" class="notice">{text("Checkout isn’t available right now. No payment was confirmed here.", "目前無法結帳，尚未確認任何付款。")}</p>}
      {entitlement()?.entitled ? <Link class="button primary" href="/dashboard">Manage Pro <ArrowRight size={16}/></Link> : !session() ? <>
        <Link class="button primary" href={carryCampaign(signInHref(returnPath()))} aria-describedby="pricing-sign-in-next"
          onClick={event => {
            if (!event.isTrusted || event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
            analytics.trackEvent("pricing_buy_clicked", { entry_surface: entrySurface(), currency: currency(), locale: locale() });
          }}>{text(`Get Lockdocs Pro — ${annual()}/yr`, `購買 Lockdocs Pro — ${annual()}／年`)} <ArrowRight size={16}/></Link>
        <p id="pricing-sign-in-next" class="pricing-sign-in-next quiet">{text("Sign in first, then return here to continue to checkout.", "請先登入，回到此頁後即可繼續結帳。")}</p>
        <PricingDecisionProof/>
      </> : details()?.billing && entitlement()?.status === "not_entitled" ? <form method="post" action="/api/billing/checkout" onSubmit={event => {
          const form = event.currentTarget;
          // Resubmits reuse the intent, so Money opens one session per attempt. Without one, checkout works as before.
          try {
            const field = form.elements.namedItem(checkoutIntentField);
            if (field instanceof HTMLInputElement) {
              field.value = attemptIntent(currency() + ":" + entrySurface(), continuesAttempt(query.billing, query.attempt), checkoutPage);
              field.disabled = false;
            }
          } catch { /* A native form submit must never depend on the intent. */ }
          analytics.beginCheckout(form, currency(), entrySurface());
        }}>
        <input type="hidden" name={checkoutIntentField} disabled/>
        <input type="hidden" name="signal_blocked" value="1"/>
        <input type="hidden" name="signal_touch" disabled/>
        <input type="hidden" name="currency" value={currency()}/>
        <input type="hidden" name="entry_surface" value={entrySurface()}/>
        <button class="button primary" type="submit">Continue to secure checkout <ArrowRight size={16}/></button>
      </form> : <p class="notice">{text("Checkout isn’t available in this environment yet.", "目前無法在此結帳。")}</p>}
      </article>
      <article class="surface plan"><div class="plan-label">{text("OPEN SOURCE", "開放原始碼")}</div><h2>{text("Free", "免費版")}</h2><div class="plan-price">{money(0)}</div><p class="quiet">{text("No account required. Runs on your machine.", "不必註冊，直接在本機執行。")}</p><ul class="check-list"><For each={[text("Exact-version dependency resolution", "確認相依套件的確切版本"), text("Local docs and symbol lookup", "在本機查閱文件與符號"), text("Three-tool MCP server for coding agents", "為程式開發代理工具提供三個 MCP 工具")]}>{(item) => <li><Check size={17}/>{item}</li>}</For></ul><Link class="button secondary" href={locale() === "en" ? "/guide" : href("/docs")}>{text("Install Lockdocs", "安裝 Lockdocs")} <ArrowRight size={16}/></Link></article>
    </div>
    <PricingProof/>
    <section class="terms"><LockKeyhole size={19}/><div><h2>{text("Annual billing and cancellation", "年費與取消訂閱")}</h2><p>{text("The full annual amount is billed upfront. UK and EU consumers generally have 14 days from purchase to cancel a new subscription. See the", "年費在訂閱時一次收取。英國與歐盟的消費者通常可在購買後 14 天內取消新訂閱。取消方式及適用的例外情況請見")} <Link href="/legal/terms">{text("subscription terms", "訂閱條款")}</Link> {text("for how to cancel and when exceptions apply. Manage renewal and invoices in your", "。你可以在")} <Link href="/dashboard">{text("account", "帳戶")}</Link>{text(".", "管理續訂與帳單。")}</p><p class="mono quiet">{currencies.map(code => money(annualAmount(code), code) + text("/yr", "／年")).join(" · ")}{text(" · Tax included", "（已含稅）")}</p></div></section>
    <EditorialFaq url={pricingUrl()} heading={text("Frequently asked questions", "常見問題")} items={pricingFaq(locale(), annual(), carryCampaign)}/>
  </div></Suspense>
  </>;
}

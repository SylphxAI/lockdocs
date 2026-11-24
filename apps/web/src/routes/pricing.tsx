import { EditorialFaq } from "@/components/editorial";
import { JsonLd } from "@/components/json-ld";
import { canonicalOrigin } from "@/lib/indexing";
import { useSiteLanguage } from "@/lib/site-language";
import { lockdocsApplication, freeOffer } from "@/lib/storefront-schema";
import { createMemo, createSignal, For, onMount, Suspense } from "solid-js";
import { createAsync, useSearchParams } from "@solidjs/router";
import { account } from "@/lib/account-query";
import { PageMeta } from "@/components/page-meta";
import { A as Link } from "@solidjs/router";
import { ArrowRight, Check, LockKeyhole } from "lucide-solid";
import { pricingCheckoutSurface, proPrice as price } from "@/lib/analytics-events";
import { pricingPath, signInHref } from "@/lib/auth-entry";
import { useCookieSettings } from "@/components/cookie-settings";

const currencies = ["USD", "GBP", "EUR", "HKD"] as const;
const prefixes = { USD: "US$", GBP: "£", EUR: "€", HKD: "HK$" };
const annualAmount = (code: typeof currencies[number]) => Number(price.unit_amounts[code]) / 100;

export default function Pricing() {
  const { href, text, locale } = useSiteLanguage();
  const pricingUrl = () => canonicalOrigin + href("/pricing");
  const currencyHref = (code: string) => pricingPath(code).replace(/^\/pricing/, href("/pricing"));
  const [query] = useSearchParams();
  const analytics = useCookieSettings();
  const currency = () => currencies.find((code) => code === query.currency) ?? "USD";
  const amount = () => annualAmount(currency());
  // SSR and the first client render agree. After hydration, follow the browser's locale.
  const [locales, setLocales] = createSignal<readonly string[]>(["en"]);
  onMount(() => setLocales(navigator.languages));
  const numberLocale = () => locale() === "en" ? locales() : locale();
  const whole = createMemo(() => new Intl.NumberFormat(numberLocale()));
  const rounded = createMemo(() => new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }));
  const money = (value: number, code = currency(), fractional = false) =>
    prefixes[code] + (fractional ? rounded() : whole()).format(value);
  const annual = () => money(amount());
  const returnPath = () => pricingPath(currency());
  const questions = () => [
    { question: text("How much does Lockdocs Pro cost?", "Lockdocs Pro 要多少錢？"), answer: text("Lockdocs Pro costs " + annual() + " per year, including tax. You pay the full annual amount upfront.", "Lockdocs Pro 年費為 " + annual() + "，已含稅，訂閱時一次付清。") },
    { question: text("Do I need an account to use Lockdocs Free?", "使用免費版需要帳戶嗎？"), answer: text("No. Exact-version docs, symbol lookup and the local MCP server are free to use without an account. You sign in to buy or manage Pro.", "不用。查閱指定版本的文件、查詢符號和使用本機 MCP 伺服器都免費，也不用登入。購買或管理 Pro 時才需要登入。") },
  ];
  const monthly = () => money(amount() / 12, currency(), amount() % 12 !== 0);
  const details = createAsync(() => account(), { deferStream: true });
  const session = () => details()?.user;
  const entitlement = () => details()?.entitlement;
  return <>
    <PageMeta title={text("Pricing · Lockdocs", "價格 · Lockdocs")} description={text("Compare Free and Lockdocs Pro, with tax-inclusive annual pricing in four currencies.", "比較免費版與 Lockdocs Pro。年費已含稅，支援四種付款幣別。")}/>
    <JsonLd data={lockdocsApplication(pricingUrl(), [freeOffer(currency()), {
      "@type": "Offer",
      name: "Lockdocs Pro",
      url: canonicalOrigin + currencyHref(currency()),
      price: amount(),
      priceCurrency: currency(),
      priceSpecification: {
        "@type": "UnitPriceSpecification",
        price: amount(),
        priceCurrency: currency(),
        billingDuration: "P1Y",
        valueAddedTaxIncluded: true,
      },
    }])}/>
    <Suspense><div class="wrap content-section">
    <div class="page-intro"><div class="kicker"><span/> {text("PLANS / 01", "方案 / 01")}</div><h1>{text("Pay for the hard parts.", "進階功能，留給棘手的升級")}</h1><p>{text("Exact-version docs are free. Pro is for the upgrades and private dependencies you can’t leave to guesswork.", "免費查閱指定版本的文件。升級套件或使用私有相依套件時，Pro 讓你有據可查。")}</p></div>
    <nav class="segmented currency-picker" aria-label={text("Billing currency", "付款幣別")}><For each={currencies}>{(code) => <a aria-current={code === currency() ? "page" : undefined} href={currencyHref(code)}>{code}</a>}</For></nav>
    <div class="plan-grid">
      <article class="surface plan"><div class="plan-label">{text("01 / OPEN SOURCE", "01 / 開放原始碼")}</div><h2>{text("Free", "免費版")}</h2><div class="plan-price">{money(0)}</div><p class="quiet">{text("No account required. Runs on your machine.", "不必註冊，直接在本機執行。")}</p><ul class="check-list"><For each={[text("Exact-version dependency resolution", "確認相依套件的確切版本"), text("Local docs and symbol lookup", "在本機查閱文件與符號"), text("Three-tool MCP server for coding agents", "為程式開發代理工具提供三個 MCP 工具")]}>{(item) => <li><Check size={17}/>{item}</li>}</For></ul><Link class="button secondary" href={locale() === "en" ? "/guide" : href("/docs")}>{text("Install Lockdocs", "安裝 Lockdocs")} <ArrowRight size={16}/></Link></article>
      <article class="surface plan plan-pro"><div class="plan-label">02 / PRO</div><h2>Lockdocs Pro</h2><div class="plan-price">{monthly()}<span>{text("/mo billed yearly", "／月（按年收費）")}</span></div><p class="plan-total">{annual()}{text("/yr upfront · Tax included", "／年，一次付清，已含稅")}{amount() % 12 !== 0 ? text(" · Monthly equivalent rounded", "；月均金額已四捨五入") : ""}</p><ul class="check-list"><For each={[text("Everything in Free", "包含免費版的所有功能"), text("Upgrade Reports with call-site citations", "升級報告，附上受影響的呼叫位置"), text("Private Package Docs", "私有套件文件")]}>{(item) => <li><Check size={17}/>{item}</li>}</For></ul>
      {query.billing === "unavailable" && <p role="status" class="notice">{text("Checkout isn’t available right now. No payment was confirmed here.", "目前無法結帳，尚未確認任何付款。")}</p>}
      {entitlement()?.entitled ? <Link class="button primary" href="/dashboard">Manage Pro <ArrowRight size={16}/></Link> : !session() ? <Link class="button primary" href={signInHref(returnPath())}>Get Lockdocs Pro — {annual()}/yr <ArrowRight size={16}/></Link> : details()?.billing && entitlement()?.status === "not_entitled" ? <form method="post" action="/api/billing/checkout" onSubmit={event => {
        const input = event.currentTarget.elements.namedItem("checkout_consent");
        if (input instanceof HTMLInputElement) input.value = analytics.checkoutChoice();
        analytics.beginCheckout(currency());
      }}>
        <input type="hidden" name="checkout_consent" value="reject"/>
        <input type="hidden" name="currency" value={currency()}/>
        <input type="hidden" name="entry_surface" value={pricingCheckoutSurface}/>
        <button class="button primary" type="submit">Continue to secure checkout <ArrowRight size={16}/></button>
      </form> : <p class="notice">{text("Checkout isn’t available in this environment yet.", "目前無法在此結帳。")}</p>}
      </article>
    </div>
    <section class="terms"><LockKeyhole size={19}/><div><h2>{text("Annual billing and cancellation", "年費與取消訂閱")}</h2><p>{text("The full annual amount is billed upfront. UK and EU consumers generally have 14 days from purchase to cancel a new subscription. See the", "年費在訂閱時一次收取。英國與歐盟的消費者通常可在購買後 14 天內取消新訂閱。取消方式及適用的例外情況請見")} <Link href="/legal/terms">{text("subscription terms", "訂閱條款")}</Link> {text("for how to cancel and when exceptions apply. Manage renewal and invoices in your", "。你可以在")} <Link href="/dashboard">{text("account", "帳戶")}</Link>{text(".", "管理續訂與帳單。")}</p><p class="mono quiet">{currencies.map(code => money(annualAmount(code), code) + text("/yr", "／年")).join(" · ")}{text(" · Tax included", "（已含稅）")}</p></div></section>
    <EditorialFaq url={pricingUrl()} heading={text("Frequently asked questions", "常見問題")} items={questions()}/>
  </div></Suspense>
  </>;
}

import { EditorialFaq } from "@/components/editorial";
import { JsonLd } from "@/components/json-ld";
import { canonicalOrigin } from "@/lib/indexing";
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

const pricingUrl = canonicalOrigin + "/pricing";
const currencies = ["USD", "GBP", "EUR", "HKD"] as const;
const prefixes = { USD: "US$", GBP: "£", EUR: "€", HKD: "HK$" };
const annualAmount = (code: typeof currencies[number]) => Number(price.unit_amounts[code]) / 100;

export default function Pricing() {
  const [query] = useSearchParams();
  const analytics = useCookieSettings();
  const currency = () => currencies.find((code) => code === query.currency) ?? "USD";
  const amount = () => annualAmount(currency());
  // SSR and the first client render agree. After hydration, follow the browser's locale.
  const [locales, setLocales] = createSignal<readonly string[]>(["en"]);
  onMount(() => setLocales(navigator.languages));
  const whole = createMemo(() => new Intl.NumberFormat(locales()));
  const rounded = createMemo(() => new Intl.NumberFormat(locales(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }));
  const money = (value: number, code = currency(), fractional = false) =>
    prefixes[code] + (fractional ? rounded() : whole()).format(value);
  const annual = () => money(amount());
  const returnPath = () => pricingPath(currency());
  const questions = () => [
    { question: "How much does Lockdocs Pro cost?", answer: "Lockdocs Pro costs " + annual() + " per year, including tax. You pay the full annual amount upfront." },
    { question: "Do I need an account to use Lockdocs Free?", answer: "No. Exact-version docs, symbol lookup and the local MCP server are free to use without an account. You sign in to buy or manage Pro." },
  ];
  const monthly = () => money(amount() / 12, currency(), amount() % 12 !== 0);
  const details = createAsync(() => account(), { deferStream: true });
  const session = () => details()?.user;
  const entitlement = () => details()?.entitlement;
  return <>
    <PageMeta title="Pricing · Lockdocs" description="Compare Free and Lockdocs Pro, with tax-inclusive annual pricing in four currencies."/>
    <JsonLd data={lockdocsApplication(pricingUrl, [freeOffer(currency()), {
      "@type": "Offer",
      name: "Lockdocs Pro",
      url: canonicalOrigin + returnPath(),
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
    <div class="page-intro"><div class="kicker"><span/> PLANS / 01</div><h1>Pay for the hard parts.</h1><p>Exact-version docs are free. Pro is for the upgrades and private dependencies you can’t leave to guesswork.</p></div>
    <nav class="segmented currency-picker" aria-label="Billing currency"><For each={currencies}>{(code) => <a aria-current={code === currency() ? "page" : undefined} href={pricingPath(code)}>{code}</a>}</For></nav>
    <div class="plan-grid">
      <article class="surface plan"><div class="plan-label">01 / OPEN SOURCE</div><h2>Free</h2><div class="plan-price">{money(0)}</div><p class="quiet">No account required. Runs on your machine.</p><ul class="check-list"><For each={["Exact-version dependency resolution", "Local docs and symbol lookup", "Three-tool MCP server for coding agents"]}>{(item) => <li><Check size={17}/>{item}</li>}</For></ul><Link class="button secondary" href="/guide">Install Lockdocs <ArrowRight size={16}/></Link></article>
      <article class="surface plan plan-pro"><div class="plan-label">02 / PRO</div><h2>Lockdocs Pro</h2><div class="plan-price">{monthly()}<span>/mo billed yearly</span></div><p class="plan-total">{annual()}/yr upfront · Tax included{amount() % 12 !== 0 ? " · Monthly equivalent rounded" : ""}</p><ul class="check-list"><For each={["Everything in Free", "Upgrade Reports with call-site citations", "Private Package Docs"]}>{(item) => <li><Check size={17}/>{item}</li>}</For></ul>
      {query.billing === "unavailable" && <p role="status" class="notice">Checkout isn’t available right now. No payment was confirmed here.</p>}
      {entitlement()?.entitled ? <Link class="button primary" href="/dashboard">Manage Pro <ArrowRight size={16}/></Link> : !session() ? <Link class="button primary" href={signInHref(returnPath())}>Get Lockdocs Pro — {annual()}/yr <ArrowRight size={16}/></Link> : details()?.billing && entitlement()?.status === "not_entitled" ? <form method="post" action="/api/billing/checkout" onSubmit={event => {
        const input = event.currentTarget.elements.namedItem("checkout_consent");
        if (input instanceof HTMLInputElement) input.value = analytics.checkoutChoice();
        analytics.beginCheckout(currency());
      }}>
        <input type="hidden" name="checkout_consent" value="reject"/>
        <input type="hidden" name="currency" value={currency()}/>
        <input type="hidden" name="entry_surface" value={pricingCheckoutSurface}/>
        <button class="button primary" type="submit">Continue to secure checkout <ArrowRight size={16}/></button>
      </form> : <p class="notice">Checkout isn’t available in this environment yet.</p>}
      </article>
    </div>
    <section class="terms"><LockKeyhole size={19}/><div><h2>Annual billing and cancellation</h2><p>The full annual amount is billed upfront. UK and EU consumers generally have 14 days from purchase to cancel a new subscription. See the <Link href="/legal/terms">subscription terms</Link> for how to cancel and when exceptions apply. Manage renewal and invoices in your <Link href="/dashboard">account</Link>.</p><p class="mono quiet">{currencies.map(code => money(annualAmount(code), code) + "/yr").join(" · ")} · Tax included</p></div></section>
    <EditorialFaq url={pricingUrl} items={questions()}/>
  </div></Suspense>
  </>;
}

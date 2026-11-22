import { LegalPage } from "@/components/legal-page";

export default function Privacy() {
  return <LegalPage title="Privacy" description="What Lockdocs keeps on your device, and what we process when you use this site.">
    <p>Updated <time datetime="2026-11-22">22 November 2026</time>.</p>
    <h2>What stays local</h2>
    <p>The browser playground processes pasted lockfiles on your device. It does not upload them. You can read the docs and use the playground without an account.</p>
    <h2>What we process</h2>
    <p>Sylphx Limited is the controller of account, billing and website analytics data for Lockdocs. We use your email, account identifier and any name you provide to operate your account. An essential session cookie keeps you signed in. If you buy Pro, the payment provider processes your payment; we use billing records to manage the subscription and meet accounting obligations.</p>
    <p>We may process request metadata such as IP addresses to operate and secure the site. If you contact support, we use the information you send to respond.</p>
    <h2>Optional analytics and advertising measurement</h2>
    <p>Lockdocs uses Google Analytics 4 to measure visits to Pricing, starts of checkout and payments confirmed for a signed-in account. For a confirmed purchase we send the amount, currency, product and a random analytics reference to avoid double-counting. We don’t send your name, email, account ID, checkout or receipt ID, or full browsing URLs, query strings or referrers in these browser events. If enabled, Google receives connection and browser information and may set analytics or ad measurement cookies.</p>
    <p>When you start checkout and analytics is allowed, our server sends the Money checkout ID, your consent settings, and available Google Analytics client and session IDs to Sylphx Events. This lets us match a completed payment to the Pricing visit even if you don’t return to Lockdocs. We send the fixed Pricing page address, without any query string or referrer. Only when ad measurement is also allowed, we may send Google ad click IDs and available Meta ad measurement cookie IDs. We may keep Google ad click IDs in a first-party cookie for up to 24 hours so they survive sign-in. We do not send a binding if you reject analytics or Global Privacy Control is on.</p>
    <p>We use the country reported by Cloudflare for this site to apply the right default. In the European Economic Area, United Kingdom and Switzerland, or when we can’t determine your country, optional features stay off until you choose to allow them. Elsewhere, analytics and ad measurement are on by default unless you opt out. Your browser’s Global Privacy Control overrides these settings and blocks both. We never enable ad personalization.</p>
    <p>Open <strong>Cookie settings</strong> in the footer to reject optional cookies, allow analytics only, or allow both analytics and ad measurement. Your explicit choice is stored in this browser for up to 180 days. Rejecting stops new Google Analytics requests from Lockdocs and removes its analytics cookies on this site. It does not erase data previously sent. If analytics is allowed when you start checkout, Lockdocs may also use a short-lived first-party cookie (up to 24 hours) to match a confirmed payment to that checkout.</p>
    <p>Read <a href="https://policies.google.com/technologies/partner-sites">how Google uses data from sites that use its services</a>.</p>
    <h2>Your rights and choices</h2>
    <p>See the <a href="https://sylphx.com/privacy">Sylphx Privacy Policy</a> for lawful bases, retention, service providers, international transfers and how to exercise your privacy rights.</p>
  </LegalPage>;
}

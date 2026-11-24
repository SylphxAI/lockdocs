import { createSignal, onMount } from "solid-js";
import { LegalPage } from "@/components/legal-page";

const updatedOn = "2026-11-23";

export default function Privacy() {
  const [updatedLabel, setUpdatedLabel] = createSignal("23 November 2026");
  // A calendar date should not change when viewed from a different time zone.
  onMount(() => setUpdatedLabel(new Intl.DateTimeFormat(navigator.languages, {
    dateStyle: "long", timeZone: "UTC",
  }).format(new Date(updatedOn))));
  return <LegalPage title="Privacy" description="What Lockdocs keeps on your device, and what we process when you use this site.">
    <p>Updated <time datetime={updatedOn}>{updatedLabel()}</time>.</p>
    <h2>What stays local</h2>
    <p>The browser playground processes pasted lockfiles on your device. It does not upload them. You can read the docs and use the playground without an account.</p>
    <h2>What we process</h2>
    <p>Sylphx Limited is the controller of account, billing and website analytics data for Lockdocs. We use your email, account identifier and any name you provide to operate your account. An essential session cookie keeps you signed in. If you buy Pro, the payment provider processes your payment; we use billing records to manage the subscription and meet accounting obligations.</p>
    <p>We may process request metadata such as IP addresses to operate and secure the site. If you contact support, we use the information you send to respond.</p>
    <h2>Optional analytics and advertising measurement</h2>
    <p>Lockdocs uses Google Analytics 4 to measure visits to known Lockdocs pages, new account sign-ups, and starts of checkout in the browser. For sign-ups we report only the sign-in method and where sign-in started. We don’t send your name, email, account ID, checkout or receipt ID, or query strings or fragments. Page addresses and referrers are limited to known Lockdocs pages; we do not send external referrers. If enabled, Google receives connection and browser information and may set analytics or ad measurement cookies.</p>
    <p>When you start checkout, our server sends the Money checkout ID and your effective consent settings to Sylphx Events. If analytics is allowed, we may also send Google Analytics client and session IDs and the Pricing page address with supported campaign parameters (UTMs), but no referrer or other query parameters. If ad measurement is allowed, we may also send Google and Meta click or measurement IDs. To keep a campaign touch through sign-in, we may store it in this browser for up to 30 days. Confirmed payments are reported to Google Analytics through Sylphx Events on the server when analytics is allowed, not by your browser when you return from checkout.</p>
    <p>We use an approximate country or region from our website network to apply cookie settings. In the European Economic Area, United Kingdom and Switzerland, or if we cannot determine your region, optional tracking stays off until you allow it. In other recognized regions, we allow analytics, ad measurement and ad personalization unless you change your settings or use Global Privacy Control. Global Privacy Control overrides your settings and turns off all optional tracking.</p>
    <p>Open <strong>Cookie settings</strong> in the footer to reject optional cookies, allow analytics only, or allow analytics, ad measurement and ad personalization. Your explicit choice is stored in this browser for up to 180 days. Rejecting stops new Google Analytics requests from Lockdocs and removes its optional tracking cookies on this site. It does not erase data previously sent.</p>
    <p>Read <a href="https://policies.google.com/technologies/partner-sites">how Google uses data from sites that use its services</a>.</p>
    <h2>Your rights and choices</h2>
    <p>See the <a href="https://sylphx.com/legal/privacy">Sylphx Privacy Policy</a> for lawful bases, retention, service providers, international transfers and how to exercise your privacy rights. To stop Pro from renewing or learn about other cancellation rights, see the <a href="/legal/terms">Lockdocs subscription terms</a>.</p>
  </LegalPage>;
}

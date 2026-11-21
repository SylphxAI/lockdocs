import { LegalPage } from "@/components/legal-page";

export default function Privacy() {
  return <LegalPage title="Privacy" description="What Lockdocs keeps on your device, and what we process when you use this site.">
    <h2>What stays local</h2>
    <p>The browser playground processes pasted lockfiles on your device. It does not upload them. You can read the docs and use the playground without an account.</p>
    <h2>What we process</h2>
    <p>Sylphx Limited is the controller of account and billing data for Lockdocs. We use your email, account identifier and any name you provide to operate your account. An essential session cookie keeps you signed in. If you buy Pro, the payment provider processes your payment; we use billing records to manage the subscription and meet accounting obligations.</p>
    <p>We may process request metadata such as IP addresses to operate and secure the site. If you contact support, we use the information you send to respond.</p>
    <h2>Your rights and choices</h2>
    <p>See the <a href="https://sylphx.com/privacy">Sylphx Privacy Policy</a> for lawful bases, retention, service providers, international transfers and how to exercise your privacy rights.</p>
  </LegalPage>;
}

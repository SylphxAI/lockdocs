import { A } from "@solidjs/router";
import { LegalPage } from "@/components/legal-page";

export default function Terms() {
  return <LegalPage title="Terms of service" description="Who provides Lockdocs, how Pro renews, and how to cancel.">
    <h2>Who you contract with</h2>
    <p>Sylphx Limited provides the Lockdocs web service. We are registered in England and Wales (company no. 16438428), at 128 City Road, London EC1V 2NX.</p>
    <p>The open-source CLI and MCP server remain governed by their repository licence. See the <a href="https://sylphx.com/terms">Sylphx Terms of Service</a> for general account and service terms.</p>
    <h2>Pro subscription</h2>
    <p>Pro is billed annually in advance. The total shown at checkout includes applicable taxes. Your subscription renews annually unless you cancel before renewal. Use billing settings in <A href="/dashboard">Account</A> to stop renewal; access normally continues to the end of the paid period.</p>
    <h2>UK and EU consumer cancellation</h2>
    <p>If you are a UK or EU consumer, you may cancel a new paid subscription within 14 days of purchase without giving a reason. To exercise this right, <a href="https://sylphx.com/contact">contact Sylphx support</a> and tell us you want to cancel.</p>
    <p>For ongoing services, a proportionate charge may apply only if you expressly requested immediate performance and the legal conditions are met. For a purchase that is legally digital content, the right can end on supply only if you expressly consented, acknowledged losing the right, and received the required confirmation. These terms do not limit your statutory rights.</p>
  </LegalPage>;
}

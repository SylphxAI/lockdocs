import { For, Suspense } from "solid-js";
import { createAsync, useSearchParams } from "@solidjs/router";
import { account } from "@/lib/account-query";
import { PageMeta } from "@/components/page-meta";
import { A as Link } from "@solidjs/router";
import { ArrowRight, BadgeCheck, CreditCard, Terminal } from "lucide-solid";
import { SignIn } from "@/components/sign-in";

const notices: Record<string, string> = {
  "billing:return": "Back from checkout. Pro access updates once payment is confirmed. Reload this page if it hasn’t appeared yet.",
  "billing:no_customer": "There’s no billing profile for this account yet. If you bought Pro with a different sign-in, switch to that account.",
  "billing:unavailable": "Billing isn’t reachable right now. Try again later.",
  "auth:failed": "Sign-in didn’t finish. Choose a provider to try again.",
  "auth:required": "Sign in before connecting the CLI.",
  "auth:sign_out_partial": "You were signed out of this site, but we couldn’t confirm the upstream session was revoked.",
  "cli:no_pro": "This account doesn’t have Pro yet. If you recently paid, wait for Pro to appear here and run lockdocs login again.",
  "cli:unavailable": "A CLI grant isn’t available for this account right now. Try again later from the terminal.",
};

export default function Dashboard() {
  const [query] = useSearchParams();
  const details = createAsync(() => account(), { deferStream: true });
  const session = () => details()?.user;
  const entitlement = () => details()?.entitlement;
  const messages = () => ["billing", "auth", "cli"].flatMap((key) => notices[key + ":" + query[key as keyof typeof query]] ?? []);
  return <Suspense><div class="wrap content-section"><PageMeta title="Account · Lockdocs" description="Manage your Lockdocs Pro subscription and connect your CLI."/>
    <div class="page-intro"><div class="kicker"><span/> YOUR WORKSPACE</div><h1>Account</h1><p>Manage Pro and connect the terminal you work in.</p></div>
    <For each={messages()}>{(text) => <p role="status" class="notice account-notice">{text}</p>}</For>
    <div class="plan-grid account-grid">
      <section class="surface plan"><div class="plan-label"><BadgeCheck size={17}/> IDENTITY &amp; ACCESS</div>
        {session() ? <><h2>{session()?.name || "Signed in"}</h2><p>Plan: <strong>{entitlement()?.status === "unavailable" ? "Couldn’t check right now" : entitlement()?.entitled ? "Lockdocs Pro · Active" : "Free"}</strong></p><div class="inline-actions">{!entitlement()?.entitled && <Link class="button primary" href="/pricing">See plans <ArrowRight size={15}/></Link>}<form action="/api/auth/sign-out" method="post"><button class="button secondary">Sign out</button></form></div></> : <><h2>Sign in to Lockdocs</h2><p class="quiet">Use the same sign-in you use for Pro.</p><SignIn capabilities={details()?.auth} next="/dashboard"/></>}
      </section>
      <section class="surface plan"><div class="plan-label"><CreditCard size={17}/> SUBSCRIPTION</div><h2>Billing &amp; recovery</h2><p class="quiet">View invoices, manage renewal, or find a purchase with the account you used at checkout.</p>
        {session() && entitlement()?.status !== "unavailable" ? <form method="post" action="/api/billing/portal"><button type="submit" class="button secondary">Open Billing Portal <ArrowRight size={15}/></button></form> : <p class="quiet">{session() ? "Billing isn’t available right now." : "Sign in to manage billing."}</p>}
      </section>
      <section class="surface plan account-cli"><div class="plan-label"><Terminal size={17}/> TERMINAL</div><h2>Connect this CLI</h2><p>With Pro active, run <code>lockdocs login</code> in your terminal and approve access in the browser. You won’t need to copy anything back.</p></section>
    </div>
  </div></Suspense>;
}

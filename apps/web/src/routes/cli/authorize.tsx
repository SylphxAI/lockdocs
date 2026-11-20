import { For, Suspense } from "solid-js";
import { createAsync, useSearchParams } from "@solidjs/router";
import { account } from "@/lib/account-query";
import { PageMeta } from "@/components/page-meta";
import { A as Link } from "@solidjs/router";
import { Terminal } from "lucide-solid";
import { SignIn } from "@/components/sign-in";
import { readConsent } from "@/lib/cli-consent";


export default function Authorize() {
  const [query] = useSearchParams();
  const consent = () => readConsent(query);
  const details = createAsync(() => account());
  const session = () => details()?.user;
  const entitlement = () => details()?.entitlement;
  const next = () => consent() ? "/cli/authorize?" + new URLSearchParams({ port: String(consent()!.port), state: consent()!.state, challenge: consent()!.challenge }) : "/dashboard";
  return <Suspense><div class="wrap content-section"><PageMeta title="Authorize the CLI · Lockdocs" description="Approve access for the Lockdocs CLI that opened this page." unlisted/><div class="surface plan consent-card"><div class="plan-label"><Terminal size={17}/> CLI AUTHORIZATION</div><h1>Connect Lockdocs</h1>
    {!consent() ? <p>This request is invalid. Run <code>lockdocs login</code> again from your terminal.</p> : <><p>Continue only if you just ran <code>lockdocs login</code> on this device. Approval sends a short-lived code to the CLI on this machine.</p>
    {!session() ? <SignIn enabled={Boolean(details()?.auth)} next={next()}/> : !entitlement()?.entitled ? <><p class="notice">{entitlement()?.status === "unavailable" ? "Couldn’t check Pro access. Try again when billing is available." : "This account doesn’t have an active Pro plan."}</p><Link href="/dashboard" class="button secondary">Back to account</Link></> :
    <form method="post" action="/api/cli/consent" class="inline-actions"><For each={Object.entries(consent() ?? {})}>{([name,value]) => <input type="hidden" name={name} value={value}/>}</For><button class="button primary">Authorize CLI</button><Link href="/dashboard" class="button secondary">Cancel</Link></form>}
    </>}
  </div></div></Suspense>;
}

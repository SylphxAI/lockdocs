import { For, Suspense } from "solid-js";
import { createAsync, useSearchParams } from "@solidjs/router";
import { account } from "@/lib/account-query";
import { PageMeta } from "@/components/page-meta";
import { A as Link } from "@solidjs/router";
import { Terminal } from "lucide-solid";
import { signInHref } from "@/lib/auth-entry";
import { readConsent } from "@/lib/cli-consent";
import { useCookieSettings } from "@/components/cookie-settings";


export default function Authorize() {
  const { trackEvent } = useCookieSettings();
  const [query] = useSearchParams();
  const consent = () => readConsent(query);
  const details = createAsync(() => account(), { deferStream: true });
  const session = () => details()?.user;
  const entitlement = () => details()?.entitlement;
  const next = () => consent() ? "/cli/authorize?" + new URLSearchParams({ port: String(consent()!.port), state: consent()!.state, challenge: consent()!.challenge }) : "/dashboard";
  return <>
    <PageMeta title="Authorize the CLI · Lockdocs" description="Approve access for the Lockdocs CLI that opened this page." unlisted/>
    <Suspense><div class="wrap content-section">
    <div class="surface plan consent-card"><div class="plan-label"><Terminal size={17}/> CLI AUTHORIZATION</div><h1>Connect Lockdocs</h1>
    {!consent() ? <p>This request is invalid. Run <code>lockdocs login</code> again from your terminal.</p> : <><p>Continue only if you just ran <code>lockdocs login</code> on this device. Approval sends a short-lived code to the CLI on this machine.</p>
    {!session() ? <Link class="button secondary auth-button" href={signInHref(next())}>Sign in</Link> : !entitlement()?.entitled ? <><p class="notice">{entitlement()?.status === "unavailable" ? "Couldn’t check Pro access. Try again when billing is available." : "This account doesn’t have an active Pro plan."}</p><Link href="/dashboard" class="button secondary">Back to account</Link></> :
    <form method="post" action="/api/cli/consent" class="inline-actions" onSubmit={() => trackEvent("cli_authorized", { entry_surface: "cli_authorize" })}><For each={Object.entries(consent() ?? {})}>{([name,value]) => <input type="hidden" name={name} value={value}/>}</For><button class="button primary">Authorize CLI</button><Link href="/dashboard" class="button secondary">Cancel</Link></form>}
    </>}
  </div></div></Suspense>
  </>;
}

import { Show } from "solid-js";
import { A, createAsync, useSearchParams } from "@solidjs/router";
import { PageMeta } from "@/components/page-meta";
import { SignIn } from "@/components/sign-in";
import { signInDetails } from "@/lib/account-query";

export default function SignInPage() {
  const [params] = useSearchParams();
  const details = createAsync(() => signInDetails(typeof params.next === "string" ? params.next : null), { deferStream: true });
  return <div class="wrap content-section">
    <PageMeta title="Sign in · Lockdocs" description="Sign in to manage your Lockdocs account." unlisted/>
    <div class="surface plan consent-card">
      <div class="plan-label">YOUR WORKSPACE</div>
      <h1>Sign in to Lockdocs</h1>
      <p class="quiet">Manage your subscription and connect the Lockdocs CLI.</p>
      <Show when={params.auth === "failed"}><p role="status" class="notice">Sign-in didn’t finish. Try again or choose another method.</p></Show>
      <Show when={details()}>{data => <Show when={!data().signedIn}
        fallback={<A class="button primary" href={data().next}>Continue</A>}>
        <SignIn capabilities={data().auth} next={data().next}/>
      </Show>}</Show>
    </div>
  </div>;
}

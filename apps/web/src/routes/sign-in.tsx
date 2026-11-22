import { createResource, createSignal, ErrorBoundary, onMount, Show, Suspense } from "solid-js";
import { A, useSearchParams } from "@solidjs/router";
import { getRequestEvent } from "solid-js/web";
import { PageMeta } from "@/components/page-meta";
import { SignIn } from "@/components/sign-in";
import { validateNext } from "@/lib/auth-entry";

async function loadSession() {
  const response = await fetch("/api/auth/session", { cache: "no-store" });
  if (!response.ok) throw new Error("Sign-in session unavailable");
  const { user } = await response.json();
  if (user !== null && (typeof user?.id !== "string" || !user.id)) throw new Error("Invalid sign-in session");
  return { signedIn: user !== null };
}

const loading = () => <p class="quiet" role="status">Loading sign-in options…</p>;

export default function SignInPage() {
  getRequestEvent()?.response.headers.set("Cache-Control", "private, no-store");
  const [params] = useSearchParams();
  const next = () => validateNext(typeof params.next === "string" ? params.next : null);
  // Use the same-origin Auth endpoint on both document loads and SPA navigation.
  // No per-user session data belongs in the server-rendered sign-in shell.
  const [mounted, setMounted] = createSignal(false);
  const [session] = createResource(mounted, loadSession);
  onMount(() => setMounted(true));

  return <div class="wrap content-section">
    <PageMeta title="Sign in · Lockdocs" description="Sign in to manage your Lockdocs account." unlisted="noindex"/>
    <div class="surface plan consent-card sign-in-card">
      <div class="plan-label">YOUR WORKSPACE</div>
      <h1>Sign in to Lockdocs</h1>
      <p class="quiet">Manage your subscription and connect the Lockdocs CLI.</p>
      <Show when={params.auth === "failed"}><p role="status" class="notice">Sign-in didn’t finish. Try again or choose another method.</p></Show>
      <ErrorBoundary fallback={<>
        <p class="notice" role="alert">Couldn’t load sign-in. Try again.</p>
        <button type="button" class="button secondary auth-button" onClick={() => window.location.reload()}>Try again</button>
      </>}>
        <Suspense fallback={loading()}><Show when={session()} fallback={loading()}>{data => <Show when={!data().signedIn}
          fallback={<A class="button primary" href={next()}>Continue</A>}>
          <SignIn next={next()}/>
        </Show>}</Show></Suspense>
      </ErrorBoundary>
    </div>
  </div>;
}

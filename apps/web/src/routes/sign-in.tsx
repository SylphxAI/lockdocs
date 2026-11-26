import { createResource, createSignal, ErrorBoundary, onMount, Show, Suspense } from "solid-js";
import { A, useSearchParams } from "@solidjs/router";
import { getRequestEvent } from "solid-js/web";
import { PageMeta } from "@/components/page-meta";
import { SignIn } from "@/components/sign-in";
import { validateNext } from "@/lib/auth-entry";
import { useSiteLanguage } from "@/lib/site-language";

async function loadSession() {
  const response = await fetch("/api/auth/session", { cache: "no-store" });
  if (!response.ok) throw new Error("Sign-in session unavailable");
  const { user } = await response.json();
  if (user !== null && (typeof user?.id !== "string" || !user.id)) throw new Error("Invalid sign-in session");
  return { signedIn: user !== null };
}

export default function SignInPage() {
  getRequestEvent()?.response.headers.set("Cache-Control", "private, no-store");
  const [params] = useSearchParams();
  const { text } = useSiteLanguage();
  const loading = () => <p class="quiet" role="status">{text("Loading sign-in options…", "正在載入登入選項…")}</p>;
  const next = () => validateNext(typeof params.next === "string" ? params.next : null);
  // Use the same-origin Auth endpoint on both document loads and SPA navigation.
  // No per-user session data belongs in the server-rendered sign-in shell.
  const [mounted, setMounted] = createSignal(false);
  const [session] = createResource(mounted, loadSession);
  onMount(() => setMounted(true));

  return <div class="wrap content-section">
    <PageMeta title={text("Sign in · Lockdocs", "登入 · Lockdocs")} description={text("Sign in to manage your Lockdocs account.", "登入以管理你的 Lockdocs 帳戶。")} unlisted="noindex"/>
    <div class="surface plan consent-card sign-in-card">
      <div class="plan-label">{text("YOUR WORKSPACE", "你的工作區")}</div>
      <h1>{text("Sign in to Lockdocs", "登入 Lockdocs")}</h1>
      <p class="quiet">{text("Manage your subscription and connect the Lockdocs CLI.", "管理訂閱並連結 Lockdocs CLI。")}</p>
      <Show when={params.auth === "failed" || params.error === "signin" || params.error === "cancelled" || params.error === "use_existing"}><p role="status" class="notice">{text("Sign-in didn’t finish. Try again or choose another method.", "未能完成登入，請重試或改用其他登入方式。")}</p></Show>
      <ErrorBoundary fallback={<>
        <p class="notice" role="alert">{text("Couldn’t load sign-in. Try again.", "無法載入登入頁面，請重試。")}</p>
        <button type="button" class="button secondary auth-button" onClick={() => window.location.reload()}>{text("Try again", "再試一次")}</button>
      </>}>
        <Suspense fallback={loading()}><Show when={session()} fallback={loading()}>{data => <Show when={!data().signedIn}
          fallback={<A class="button primary" href={next()}>{text("Continue", "繼續")}</A>}>
          <SignIn next={next()}/>
        </Show>}</Show></Suspense>
      </ErrorBoundary>
    </div>
  </div>;
}

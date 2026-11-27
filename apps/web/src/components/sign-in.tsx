import { createEffect, createSignal, onCleanup, onMount, Show, untrack } from "solid-js";
import { signInMessages } from "@sylphx/auth-solid";
import { Fingerprint } from "lucide-solid";
import type { SignInMethod } from "@/lib/auth";
import { requestPasskey, supportsPasskeys } from "@/lib/passkey-client";
import type { SignInOptions as Options } from "@/lib/sign-in-options";
import { useSiteLanguage } from "@/lib/site-language";

type EmailAction = "send" | "verify";
const mask = (address: string) => address[0] + "•••" + address.slice(address.lastIndexOf("@"));
const usable = (options: Options | null | undefined) => options && Object.values(options.capabilities).some(Boolean) ? options : undefined;

function GoogleMark() {
  return <svg width="18" height="18" viewBox="0 0 48 48" aria-hidden="true">
    <path fill="#4285F4" d="M43.6 24.5c0-1.4-.1-2.7-.4-4H24v7.6h11a9.4 9.4 0 0 1-4.1 6.2v5h6.6c3.9-3.6 6.1-8.7 6.1-14.8z"/>
    <path fill="#34A853" d="M24 44c5.5 0 10.1-1.8 13.5-4.8l-6.6-5c-1.8 1.2-4.1 2-6.9 2-5.3 0-9.8-3.6-11.4-8.4H5.8V33A20.4 20.4 0 0 0 24 44z"/>
    <path fill="#FBBC05" d="M12.6 27.8a12.4 12.4 0 0 1 0-7.6V15H5.8a20 20 0 0 0 0 18z"/>
    <path fill="#EA4335" d="M24 11.8c3 0 5.7 1 7.8 3l5.8-5.8A19.6 19.6 0 0 0 24 4 20.4 20.4 0 0 0 5.8 15l6.8 5.2c1.6-4.9 6.1-8.4 11.4-8.4z"/>
  </svg>;
}
function GitHubMark() {
  return <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 0a12 12 0 0 0-3.8 23.4c.6.1.8-.3.8-.6v-2.1c-3.3.7-4-1.4-4-1.4-.5-1.4-1.3-1.7-1.3-1.7-1.1-.8.1-.8.1-.8 1.2.1 1.8 1.2 1.8 1.2 1.1 1.8 2.9 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.7-.3-5.5-1.3-5.5-6a4.7 4.7 0 0 1 1.3-3.2c-.1-.3-.6-1.5.1-3.2 0 0 1-.3 3.3 1.2a11.4 11.4 0 0 1 6 0c2.3-1.5 3.3-1.2 3.3-1.2.7 1.7.3 2.9.1 3.2a4.6 4.6 0 0 1 1.3 3.2c0 4.7-2.8 5.7-5.5 6 .4.4.8 1.1.8 2.2v3.2c0 .3.2.7.8.6A12 12 0 0 0 12 0z"/></svg>;
}

/** Server-rendered from `initial`, so the form keeps its place when the page hydrates. */
export function SignIn(props: { next: string; initial: Options | null }) {
  const { locale, text } = useSiteLanguage();
  const messages = () => locale() === "en" ? signInMessages.en : signInMessages["zh-Hant-TW"];
  const errors = {
    options: () => text("Couldn’t load sign-in options. Try again.", "無法載入登入選項，請重試。"),
    passkey: () => text("Passkey sign-in didn’t finish. Try again or choose another method.", "未能使用通行金鑰登入，請重試或改用其他方式。"),
    rate: () => text("Too many attempts. Wait a moment and try again.", "嘗試次數過多，請稍候再試。"),
    verify: () => text("Couldn’t verify that code. Check it or request a new one.", "無法驗證這組驗證碼，請確認後再試，或要求重寄。"),
    send: () => text("Couldn’t send a code. Try again or choose another method.", "無法寄出驗證碼，請重試或改用其他登入方式。"),
    connection: () => text("Sign-in couldn’t connect. Try again.", "登入時無法連線，請重試。"),
  };
  // The first render uses the server's answer; a retry replaces it.
  const initial = untrack(() => usable(props.initial));
  const [options, setOptions] = createSignal(initial);
  const caps = () => options()?.capabilities;
  const [loading, setLoading] = createSignal(false);
  const [message, setMessage] = createSignal<keyof typeof errors | undefined>(initial ? undefined : "options");
  // Interactive controls wait for hydration; their place on the page does not.
  const [mounted, setMounted] = createSignal(false);
  const [email, setEmail] = createSignal("");
  const [step, setStep] = createSignal<"email" | "proof">("email");
  const [pending, setPending] = createSignal<EmailAction | "passkey">();
  const [code, setCode] = createSignal("");
  const [delivery, setDelivery] = createSignal<{ email: string; expiresAt: number; resendAt: number }>();
  const [now, setNow] = createSignal(0);
  const sent = () => delivery()?.email === email() && now() < delivery()!.expiresAt;
  const cooldown = () => sent() ? Math.max(0, Math.ceil((delivery()!.resendAt - now()) / 1000)) : 0;
  // Assume support until the browser answers, so a supported browser draws the same button the server did.
  const [passkeySupported, setPasskeySupported] = createSignal(true);
  const passkeys = () => caps()?.passkey && passkeySupported();
  const life = new AbortController();
  let ceremony: { cancel: AbortController; done: Promise<void> } | undefined;
  let emailField: HTMLInputElement | undefined;
  let codeField: HTMLInputElement | undefined;
  let proofHeading: HTMLHeadingElement | undefined;

  async function loadOptions() {
    setLoading(true);
    setMessage(undefined);
    try {
      const response = await fetch("/api/auth/options", { cache: "no-store", signal: life.signal });
      if (!response.ok) throw new Error();
      const data = usable(await response.json());
      if (!data) throw new Error();
      setOptions(data);
    } catch {
      if (!life.signal.aborted) setMessage("options");
    } finally { setLoading(false); }
  }

  function startPasskey(mediation: "conditional" | "required") {
    const cancel = new AbortController();
    const done = (async () => {
      try {
        const assertion = await requestPasskey(mediation, cancel.signal);
        if (!assertion || cancel.signal.aborted) return;
        setPending("passkey");
        const response = await fetch("/api/auth/passkey/verify", {
          method: "POST", headers: { "Content-Type": "application/json" },
          signal: life.signal, body: JSON.stringify({ ...assertion, next: props.next }),
        });
        if (!response.ok) throw new Error();
        window.location.assign((await response.json()).next);
      } catch {
        if (!cancel.signal.aborted && !life.signal.aborted && (mediation === "required" || pending() === "passkey")) {
          setMessage("passkey");
        }
      } finally {
        if (ceremony?.cancel === cancel) ceremony = undefined;
        if (pending() === "passkey") setPending(undefined);
      }
    })();
    ceremony = { cancel, done };
  }

  async function explicitPasskey() {
    if (pending()) return;
    setMessage(undefined);
    const old = ceremony;
    old?.cancel.abort();
    // A pending begin can still write its challenge cookie. Serialize replacements.
    setPending("passkey");
    await old?.done;
    if (!life.signal.aborted) {
      setPending("passkey");
      startPasskey("required");
    }
  }

  onMount(() => {
    setPasskeySupported(supportsPasskeys());
    setMounted(true);
    const interval = window.setInterval(() => setNow(Date.now()), 250);
    setNow(Date.now());
    onCleanup(() => window.clearInterval(interval));
  });
  onCleanup(() => { life.abort(); ceremony?.cancel.abort(); });
  createEffect(() => {
    if (!mounted() || !passkeys() || step() !== "email") return;
    let active = true;
    void (async () => {
      if (!(await PublicKeyCredential.isConditionalMediationAvailable?.()) || !active || pending()) return;
      await ceremony?.done;
      if (active && !pending() && !ceremony) startPasskey("conditional");
    })().catch(() => { /* Optional browser capability. */ });
    onCleanup(() => { active = false; });
  });

  async function submitProof(action: EmailAction) {
    if (pending() || (action === "verify" && (!sent() || code().length !== 6)) || (action === "send" && cooldown())) return;
    setPending(action);
    setMessage(undefined);
    try {
      const response = await fetch("/api/auth/sign-in", {
        method: "POST", headers: { "Content-Type": "application/json" }, signal: life.signal,
        body: JSON.stringify({ action, next: props.next, ...(action === "verify" ? { code: code() } : { email: email() }) }),
      });
      if (!response.ok) {
        setMessage(response.status === 429 ? "rate" : action);
        return;
      }
      const result = await response.json();
      if (result.next) window.location.assign(result.next);
      else if (action === "send" && result.step === "email_otp") {
        const time = Date.now();
        setNow(time);
        setDelivery({ email: email(), resendAt: time + 30_000, expiresAt: time + result.expiresIn * 1000 });
        setCode("");
      }
    } catch {
      if (!life.signal.aborted) setMessage("connection");
    } finally {
      setPending(undefined);
      if (action === "send") codeField?.focus();
    }
  }

  function continueWithEmail(event: SubmitEvent & { currentTarget: HTMLFormElement }) {
    event.preventDefault();
    if (pending()) return;
    ceremony?.cancel.abort();
    setEmail(String(new FormData(event.currentTarget).get("email")).trim());
    setMessage(undefined);
    setStep("proof");
    if (caps()?.email_otp && !sent()) void submitProof("send");
    const target = sent() ? codeField : proofHeading;
    queueMicrotask(() => target?.focus());
  }

  function changeEmail() {
    setMessage(undefined);
    setCode("");
    setStep("email");
    queueMicrotask(() => emailField?.focus());
  }

  function enterCode(field: HTMLInputElement, input: string) {
    const value = input.replace(/[^0-9]/g, "").slice(0, 6);
    field.value = value;
    setCode(value);
    setMessage(undefined);
    if (value.length === 6) void submitProof("verify");
  }

  const lastUsed = (method: SignInMethod) => <Show when={options()?.lastUsed === method}><span class="last-used">{messages().lastUsed}</span></Show>;
  const PasskeyButton = () => <Show when={passkeys()}>
    <button class="button quiet auth-button auth-passkey" type="button" onClick={explicitPasskey} disabled={!mounted() || Boolean(pending())}>
      <Fingerprint size={18}/>{pending() === "passkey" ? text("Waiting for passkey…", "正在等待通行金鑰…") : text("Continue with a passkey", "使用通行金鑰登入")}{lastUsed("passkey")}
    </button>
  </Show>;
  const providerHref = (provider: "google" | "github") => "/api/auth/" + provider + "?" + new URLSearchParams({ next: props.next });

  return <div class="unified-auth">
    <Show when={!loading()} fallback={<p class="quiet" role="status">{messages().loading}</p>}>
      <Show when={options()} fallback={<button class="button secondary auth-button" type="button" onClick={loadOptions}>{messages().retry}</button>}>
        <Show when={step() === "email"} fallback={<>
          <h2 class="proof-heading" tabIndex={-1} ref={proofHeading}>{caps()?.email_otp ? text("Check your email", "查看電子郵件") : text("Choose a sign-in method", "選擇登入方式")}</h2>
          <Show when={caps()?.email_otp}>
            <Show when={sent()} fallback={
              <Show when={pending() !== "send"} fallback={<p role="status" class="quiet">{text("Requesting a code…", "正在要求寄送驗證碼…")}</p>}>
                <button type="button" class="button secondary auth-button" disabled={Boolean(pending())} onClick={() => void submitProof("send")}>{text("Send me a code", "寄送驗證碼")}</button>
              </Show>
            }>
              <p class="quiet">{text("Sent to ", "已寄送至 ")}<strong>{mask(email())}</strong></p>
              <form method="post" onSubmit={event => { event.preventDefault(); void submitProof("verify"); }}>
                <label for="signin-code">{text("Six-digit code", "六位數驗證碼")}</label>
                <input id="signin-code" ref={codeField} name="code" type="text" inputmode="numeric" autocomplete="one-time-code" pattern="[0-9]{6}" value={code()} required
                  readOnly={Boolean(pending())} onInput={event => enterCode(event.currentTarget, event.currentTarget.value)}
                  onPaste={event => { if (pending()) return; event.preventDefault(); enterCode(event.currentTarget, event.clipboardData?.getData("text") ?? ""); }}/>
                <button class="button primary auth-button" type="submit" disabled={code().length !== 6 || Boolean(pending())}>{pending() === "verify" ? text("Verifying…", "驗證中…") : messages().verifyCode}</button>
              </form>
              <button class="auth-text-button" type="button" disabled={Boolean(pending()) || cooldown() > 0} onClick={() => void submitProof("send")}>{cooldown() ? text("Resend in " + cooldown() + "s", cooldown() + " 秒後可重寄") : messages().resendCode}</button>
            </Show>
          </Show>
          <PasskeyButton/>
          <button class="auth-text-button" type="button" disabled={Boolean(pending())} onClick={changeEmail}>{text("Use a different email", "改用其他電子郵件地址")}</button>
        </>}>
          <Show when={caps()?.google || caps()?.github}>
            <div class="auth-providers">
              <Show when={caps()?.google}><a rel="external" class="button outline auth-button" href={providerHref("google")}><GoogleMark/> {text("Continue with Google", "使用 Google 登入")} {lastUsed("google")}</a></Show>
              <Show when={caps()?.github}><a rel="external" class="button outline auth-button" href={providerHref("github")}><GitHubMark/> {text("Continue with GitHub", "使用 GitHub 登入")} {lastUsed("github")}</a></Show>
            </div>
          </Show>
          <Show when={caps()?.email_otp || passkeys()}>
            <Show when={caps()?.google || caps()?.github}><div class="auth-divider" aria-hidden="true">{text("or", "或")}</div></Show>
            <form method="post" onSubmit={continueWithEmail}>
              <label for="signin-email">{messages().email}</label>
              <input id="signin-email" ref={emailField} type="email" name="email" autocomplete="username webauthn" inputmode="email" maxlength="254" value={email()} required/>
              <button type="submit" class="button primary auth-button" disabled={!mounted() || Boolean(pending())}>{text("Continue", "繼續")}</button>
            </form>
            <PasskeyButton/>
          </Show>
        </Show>
      </Show>
    </Show>
    <Show when={message()}>{error => <p class="notice" role="alert">{errors[error()]()}</p>}</Show>
    <p class="auth-terms">{text("By continuing, you agree to the ", "繼續即表示你同意")}<a href="/legal/terms">{text("Terms", "服務條款")}</a>{text(" and acknowledge the ", "，並已知悉")}<a href="/legal/privacy">{text("Privacy Policy", "隱私權政策")}</a>{text(".", "。")}</p>
  </div>;
}

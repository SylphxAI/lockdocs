import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { signInMessages } from "@sylphx/auth-solid";
import { Fingerprint } from "lucide-solid";
import type { AuthCapabilities, SignInMethod } from "@/lib/auth";
import { requestPasskey, supportsPasskeys } from "@/lib/passkey-client";

type Options = { capabilities: AuthCapabilities; lastUsed: SignInMethod | null };
type EmailAction = "send" | "verify";
const messages = signInMessages.en;
const mask = (address: string) => address[0] + "•••" + address.slice(address.lastIndexOf("@"));

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

export function SignIn(props: { next: string }) {
  const [options, setOptions] = createSignal<Options>();
  const caps = () => options()?.capabilities;
  const [loading, setLoading] = createSignal(true);
  const [message, setMessage] = createSignal("");
  const [email, setEmail] = createSignal("");
  const [step, setStep] = createSignal<"email" | "proof">("email");
  const [pending, setPending] = createSignal<EmailAction | "passkey">();
  const [code, setCode] = createSignal("");
  const [delivery, setDelivery] = createSignal<{ email: string; expiresAt: number; resendAt: number }>();
  const [now, setNow] = createSignal(0);
  const sent = () => delivery()?.email === email() && now() < delivery()!.expiresAt;
  const cooldown = () => sent() ? Math.max(0, Math.ceil((delivery()!.resendAt - now()) / 1000)) : 0;
  const [passkeySupported, setPasskeySupported] = createSignal(false);
  const passkeys = () => caps()?.passkey && passkeySupported();
  const life = new AbortController();
  let ceremony: { cancel: AbortController; done: Promise<void> } | undefined;
  let emailField: HTMLInputElement | undefined;
  let codeField: HTMLInputElement | undefined;
  let proofHeading: HTMLHeadingElement | undefined;

  async function loadOptions() {
    setLoading(true);
    setMessage("");
    try {
      const response = await fetch("/api/auth/options", { cache: "no-store", signal: life.signal });
      if (!response.ok) throw new Error();
      const data: Options = await response.json();
      if (!Object.values(data.capabilities).some(Boolean)) throw new Error();
      setOptions(data);
    } catch {
      if (!life.signal.aborted) setMessage("Couldn’t load sign-in options. Try again.");
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
          setMessage("Passkey sign-in didn’t finish. Try again or choose another method.");
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
    setMessage("");
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
    void loadOptions();
    const interval = window.setInterval(() => setNow(Date.now()), 250);
    setNow(Date.now());
    onCleanup(() => window.clearInterval(interval));
  });
  onCleanup(() => { life.abort(); ceremony?.cancel.abort(); });
  createEffect(() => {
    if (!passkeys() || step() !== "email") return;
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
    setMessage("");
    try {
      const response = await fetch("/api/auth/sign-in", {
        method: "POST", headers: { "Content-Type": "application/json" }, signal: life.signal,
        body: JSON.stringify({ action, next: props.next, ...(action === "verify" ? { code: code() } : { email: email() }) }),
      });
      if (!response.ok) {
        setMessage(response.status === 429 ? "Too many attempts. Wait a moment and try again." : action === "verify"
          ? "Couldn’t verify that code. Check it or request a new one." : "Couldn’t send a code. Try again or choose another method.");
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
      if (!life.signal.aborted) setMessage("Sign-in couldn’t connect. Try again.");
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
    setMessage("");
    setStep("proof");
    if (caps()?.email_otp && !sent()) void submitProof("send");
    const target = sent() ? codeField : proofHeading;
    queueMicrotask(() => target?.focus());
  }

  function changeEmail() {
    setMessage("");
    setCode("");
    setStep("email");
    queueMicrotask(() => emailField?.focus());
  }

  function enterCode(field: HTMLInputElement, input: string) {
    const value = input.replace(/[^0-9]/g, "").slice(0, 6);
    field.value = value;
    setCode(value);
    setMessage("");
    if (value.length === 6) void submitProof("verify");
  }

  const lastUsed = (method: SignInMethod) => <Show when={options()?.lastUsed === method}><span class="last-used">{messages.lastUsed}</span></Show>;
  const PasskeyButton = () => <Show when={passkeys()}>
    <button class="button quiet auth-button auth-passkey" type="button" onClick={explicitPasskey} disabled={Boolean(pending())}>
      <Fingerprint size={18}/>{pending() === "passkey" ? "Waiting for passkey…" : "Continue with a passkey"}{lastUsed("passkey")}
    </button>
  </Show>;
  const providerHref = (provider: "google" | "github") => "/api/auth/" + provider + "?" + new URLSearchParams({ next: props.next });

  return <div class="unified-auth">
    <Show when={!loading()} fallback={<p class="quiet" role="status">{messages.loading}</p>}>
      <Show when={options()} fallback={<button class="button secondary auth-button" type="button" onClick={loadOptions}>{messages.retry}</button>}>
        <Show when={step() === "email"} fallback={<>
          <h2 class="proof-heading" tabIndex={-1} ref={proofHeading}>{caps()?.email_otp ? "Check your email" : "Choose a sign-in method"}</h2>
          <Show when={caps()?.email_otp}>
            <Show when={sent()} fallback={
              <Show when={pending() !== "send"} fallback={<p role="status" class="quiet">Requesting a code…</p>}>
                <button type="button" class="button secondary auth-button" disabled={Boolean(pending())} onClick={() => void submitProof("send")}>Send me a code</button>
              </Show>
            }>
              <p class="quiet">Sent to <strong>{mask(email())}</strong></p>
              <form method="post" onSubmit={event => { event.preventDefault(); void submitProof("verify"); }}>
                <label for="signin-code">Six-digit code</label>
                <input id="signin-code" ref={codeField} name="code" type="text" inputmode="numeric" autocomplete="one-time-code" pattern="[0-9]{6}" value={code()} required
                  readOnly={Boolean(pending())} onInput={event => enterCode(event.currentTarget, event.currentTarget.value)}
                  onPaste={event => { if (pending()) return; event.preventDefault(); enterCode(event.currentTarget, event.clipboardData?.getData("text") ?? ""); }}/>
                <button class="button primary auth-button" type="submit" disabled={code().length !== 6 || Boolean(pending())}>{pending() === "verify" ? "Verifying…" : messages.verifyCode}</button>
              </form>
              <button class="auth-text-button" type="button" disabled={Boolean(pending()) || cooldown() > 0} onClick={() => void submitProof("send")}>{cooldown() ? "Resend in " + cooldown() + "s" : messages.resendCode}</button>
            </Show>
          </Show>
          <PasskeyButton/>
          <button class="auth-text-button" type="button" disabled={Boolean(pending())} onClick={changeEmail}>Use a different email</button>
        </>}>
          <Show when={caps()?.google || caps()?.github}>
            <div class="auth-providers">
              <Show when={caps()?.google}><a rel="external" class="button outline auth-button" href={providerHref("google")}><GoogleMark/> Continue with Google {lastUsed("google")}</a></Show>
              <Show when={caps()?.github}><a rel="external" class="button outline auth-button" href={providerHref("github")}><GitHubMark/> Continue with GitHub {lastUsed("github")}</a></Show>
            </div>
          </Show>
          <Show when={caps()?.email_otp || passkeys()}>
            <Show when={caps()?.google || caps()?.github}><div class="auth-divider" aria-hidden="true">or</div></Show>
            <form method="post" onSubmit={continueWithEmail}>
              <label for="signin-email">{messages.email}</label>
              <input id="signin-email" ref={emailField} type="email" name="email" autocomplete="username webauthn" inputmode="email" maxlength="254" value={email()} required/>
              <button type="submit" class="button primary auth-button" disabled={Boolean(pending())}>Continue</button>
            </form>
            <PasskeyButton/>
          </Show>
        </Show>
      </Show>
    </Show>
    <Show when={message()}><p class="notice" role="alert">{message()}</p></Show>
    <p class="auth-terms">By continuing, you agree to the <a href="/legal/terms">Terms</a> and acknowledge the <a href="/legal/privacy">Privacy Policy</a>.</p>
  </div>;
}

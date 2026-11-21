import { For, Show, createSignal } from "solid-js";
import { GitFork } from "lucide-solid";
import type { AuthCapabilities } from "@/lib/auth";
import { PasskeyButton } from "./passkey-button";

type EmailAction = "send" | "password" | "sign_up";

function EmailForm(props: { action: EmailAction; label: string; next: string }) {
  const [step, setStep] = createSignal<"start" | "email_otp" | "check_email">("start");
  const [message, setMessage] = createSignal("");
  const [pending, setPending] = createSignal(false);
  async function submit(event: SubmitEvent & { currentTarget: HTMLFormElement }) {
    event.preventDefault();
    if (pending()) return;
    const data = Object.fromEntries(new FormData(event.currentTarget));
    const action = step() === "email_otp" ? "verify" : props.action;
    setPending(true);
    setMessage("");
    try {
      const response = await fetch("/api/auth/sign-in", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...data, action, next: props.next }),
      });
      const result = await response.json();
      if (response.ok && result.next) window.location.assign(result.next);
      else if (response.ok && (result.step === "email_otp" || result.step === "check_email")) setStep(result.step);
      else setMessage(response.status === 429 ? "Too many attempts. Wait a moment and try again." :
        action === "verify" ? "Couldn’t verify that code. Check it or request a new one." :
        "Couldn’t complete this request. Check your details or try another sign-in method.");
    } catch { setMessage("Sign-in couldn’t connect. Try again."); }
    finally { setPending(false); }
  }
  return <details class="email-choice">
    <summary class="button secondary auth-button">{props.label}</summary>
    <Show when={step() !== "check_email"} fallback={<p role="status">If you can create an account with that address, you’ll get an email to confirm it.</p>}>
      <form method="post" action="/api/auth/sign-in" onSubmit={submit}>
        <Show when={step() === "email_otp"} fallback={<>
          <label>Email<input type="email" name="email" autocomplete="email" maxlength="254" required/></label>
          <Show when={props.action !== "send"}>
            <label>Password<input type="password" name="password" autocomplete={props.action === "sign_up" ? "new-password" : "current-password"} minlength={props.action === "sign_up" ? 12 : undefined} maxlength="1024" required/></label>
          </Show>
        </>}>
          <label>Six-digit code<input name="code" inputmode="numeric" autocomplete="one-time-code" pattern="[0-9]{6}" required/></label>
        </Show>
        <Show when={message()}><p role="alert">{message()}</p></Show>
        <button type="submit" class="button primary" disabled={pending()}>{step() === "email_otp" ? "Verify code" : props.action === "send" ? "Send code" : props.label}</button>
        <Show when={step() === "email_otp"}>
          <button type="button" class="button quiet" disabled={pending()} onClick={() => { setMessage(""); setStep("start"); }}>Request another code</button>
        </Show>
      </form>
    </Show>
  </details>;
}

export function SignIn(props: { capabilities: AuthCapabilities | null | undefined; next: string }) {
  const caps = () => props.capabilities;
  const link = (provider: string) => "/api/auth/" + provider + "?" + new URLSearchParams({ next: props.next });
  const choices = [["email_otp", "send", "Continue with an email code"], ["password", "password", "Sign in with a password"], ["sign_up", "sign_up", "Create an account"]] as const;
  return <Show when={caps()?.github || caps()?.google || caps()?.passkey || caps()?.email_otp || caps()?.password}
    fallback={<div class="notice"><strong>Sign-in isn’t available in this environment yet.</strong><p>The docs and playground work without an account.</p></div>}>
    <div class="auth-choices">
      <Show when={caps()?.github}><a class="button secondary auth-button" href={link("github")}><GitFork size={17}/> Continue with GitHub</a></Show>
      <Show when={caps()?.google}><a class="button secondary auth-button" href={link("google")}><span aria-hidden="true" class="google-g">G</span> Continue with Google</a></Show>
      <Show when={caps()?.passkey}><PasskeyButton next={props.next}/></Show>
      <For each={choices}>{([capability, action, label]) => <Show when={caps()?.[capability] && (action !== "sign_up" || caps()?.password)}>
        <EmailForm action={action} label={label} next={props.next}/>
      </Show>}</For>
    </div>
  </Show>;
}

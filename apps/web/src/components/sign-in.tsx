import { Show } from "solid-js";
import { GitFork } from "lucide-solid";
import { PasskeyButton } from "./passkey-button";

export function SignIn(props: { enabled: boolean; next: string }) {
  const unavailable = <div class="notice"><strong>Sign-in isn’t available in this environment yet.</strong><p>The docs and playground work without an account.</p></div>;
  const link = (provider: string) => "/api/auth/sign-in?" + new URLSearchParams({ provider, next: props.next }).toString();
  return <Show when={props.enabled} fallback={unavailable}><div class="auth-choices">
    <a class="button primary auth-button" href={link("github")}><GitFork size={17}/> Continue with GitHub</a>
    <a class="button secondary auth-button" href={link("google")}><span aria-hidden="true" class="google-g">G</span> Continue with Google</a>
    <PasskeyButton next={props.next}/>
  </div></Show>;
}

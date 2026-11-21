

import { createSignal } from "solid-js";
import { Fingerprint } from "lucide-solid";

function toBytes(value: string) {
  return Uint8Array.from(atob(value.replaceAll("-", "+").replaceAll("_", "/")), (character) => character.charCodeAt(0));
}
function encode(value: ArrayBuffer) {
  return btoa(String.fromCharCode(...new Uint8Array(value))).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
export function PasskeyButton(props: { next: string }) {
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal("");
  async function signIn() {
    setMessage("");
    if (!window.PublicKeyCredential || !navigator.credentials) {
      setMessage("This browser cannot use a passkey here. Continue with GitHub or Google instead.");
      return;
    }
    setBusy(true);
    try {
      const begin = await fetch("/api/auth/passkey/begin", { method: "POST" });
      if (!begin.ok) throw new Error();
      const { challenge_id, rp_id, challenge, allowed_credential_ids } = await begin.json();
      const credential = await navigator.credentials.get({ publicKey: {
        challenge: toBytes(challenge), rpId: rp_id, timeout: 60_000, userVerification: "required",
        allowCredentials: allowed_credential_ids.map((id: string) => ({ type: "public-key", id: toBytes(id) })),
      } }) as PublicKeyCredential | null;
      if (!credential) throw new Error();
      const assertion = credential.response as AuthenticatorAssertionResponse;
      const verified = await fetch("/api/auth/passkey/verify", {
        method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({
          next: props.next, challenge_id, credential_id: encode(credential.rawId), client_data_json: encode(assertion.clientDataJSON),
          authenticator_data: encode(assertion.authenticatorData), signature: encode(assertion.signature),
          user_handle: assertion.userHandle ? encode(assertion.userHandle) : "",
        }),
      });
      if (!verified.ok) throw new Error();
      window.location.assign((await verified.json()).next);
    } catch {
      setMessage("Passkey sign-in didn’t finish. If you haven’t enrolled a passkey on this site, continue with GitHub or Google.");
    } finally {
      setBusy(false);
    }
  }
  return <><button class="button secondary auth-button" type="button" onClick={signIn} disabled={busy()}><Fingerprint size={17}/>{busy() ? "Waiting for passkey…" : "Continue with Passkey"}</button>{message() && <p role="status" class="quiet">{message()}</p>}</>;
}

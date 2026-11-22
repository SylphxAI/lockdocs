function bytes(value: string) {
  return Uint8Array.from(atob(value.replaceAll("-", "+").replaceAll("_", "/")), character => character.charCodeAt(0));
}

function encode(value: ArrayBuffer) {
  return btoa(String.fromCharCode(...new Uint8Array(value))).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

export function supportsPasskeys() {
  return Boolean(window.PublicKeyCredential && navigator.credentials?.get);
}

export async function requestPasskey(mediation: "conditional" | "required", signal: AbortSignal) {
  if (signal.aborted) return null;
  // Let the begin request finish setting its challenge cookie before a replacement ceremony starts.
  const begin = await fetch("/api/auth/passkey/begin", { method: "POST" });
  if (signal.aborted) return null;
  if (!begin.ok) throw new Error("Passkey unavailable");
  const { challenge_id, rp_id, challenge, allowed_credential_ids } = await begin.json();
  const credential = await navigator.credentials.get({ mediation, signal, publicKey: {
    challenge: bytes(challenge), rpId: rp_id, timeout: 60_000, userVerification: "required",
    allowCredentials: mediation === "conditional" ? [] : allowed_credential_ids.map((id: string) => ({ type: "public-key", id: bytes(id) })),
  } }) as PublicKeyCredential | null;
  if (!credential || signal.aborted) return null;
  const response = credential.response as AuthenticatorAssertionResponse;
  return {
    challenge_id, credential_id: encode(credential.rawId), client_data_json: encode(response.clientDataJSON),
    authenticator_data: encode(response.authenticatorData), signature: encode(response.signature),
    user_handle: response.userHandle ? encode(response.userHandle) : "",
  };
}

import { readAuthSessionCookie, type SessionAdapter, type VerifiedIdentitySession } from "@sylphx/auth";
import type { EntryConfig } from "@sylphx/auth/client";
import { timingSafeEqual } from "node:crypto";
import {
  authHeaders, clientEndpoint, createIdentityHandler, inspectRequestSession, exchangeMoneyToken,
  OAUTH, oauthSessionAge, providerFetch, redeem, routePrefix, SESSION,
  type AuthCapabilities, type AuthConfig, type OAuthProvider, type Session, type SignInMethod,
} from "./auth-identity";
import { cookie, httpsOrigin, setPrivateCookie } from "./http";
import { boundedJson } from "./bounded-json";
import { clearSignupMarker, markSignup } from "./signup-analytics";
import { hasPublicOrigin } from "./public-origin";
import { isCliCode, validateNext } from "./auth-entry";

export { httpsOrigin } from "./http";
export { endUserId, isVerifiedSession, providerFetch } from "./auth-identity";
export type { AuthCapabilities, OAuthProvider, Session, SignInMethod } from "./auth-identity";

const EMAIL_FLOW = "__Host-lockdocs-email-flow";
const PASSKEY = "__Host-lockdocs-passkey";
const LAST_USED = "__Host-lockdocs-last-auth-method";
const lastUsedAge = 90 * 24 * 60 * 60;
const privateHeaders = { "Cache-Control": "no-store", "Referrer-Policy": "no-referrer" };
const base64url = /^[A-Za-z0-9_-]+$/;

export function appOrigin() {
  return httpsOrigin(process.env.LOCKDOCS_APP_ORIGIN);
}

function config(): AuthConfig | null {
  const origin = appOrigin();
  const base = httpsOrigin(process.env.SYLPHX_AUTH_URL);
  const key = process.env.SYLPHX_PUBLISHABLE_KEY;
  const secret = process.env.SYLPHX_AUTH_SECRET_KEY ?? process.env.SYLPHX_SECRET_KEY;
  const instance = process.env.SYLPHX_AUTH_ORGANIZATION_ID;
  return origin && base && key && secret && instance ? { origin, base, key, secret, instance } : null;
}

export function authConfigured(): boolean {
  return config() !== null;
}

/** The token is a private Auth session credential: never serialize it to the client. */
export async function getSession(request: Request): Promise<Session | null> {
  return inspectRequestSession(request, config());
}

/** Exchange only a verified direct session; OAuth credentials cannot open Money's end-user routes. */
export async function moneyAccessToken(session: Session): Promise<string | null> {
  return exchangeMoneyToken(session, config());
}

function sdk(auth: AuthConfig, session?: SessionAdapter) {
  return createIdentityHandler(auth, process.env.LOCKDOCS_TENANT_AUTH_ORIGIN, session);
}

// Keep the existing options shape for the Lockdocs sign-in view.
export async function getCapabilities(request: Request, auth = config()): Promise<AuthCapabilities | null> {
  if (!auth) return null;
  try {
    const url = new URL(request.url);
    url.pathname = routePrefix + "/config";
    const response = await sdk(auth)(new Request(url, { headers: request.headers, signal: request.signal }));
    if (!response.ok) return null;
    const entry: EntryConfig = await response.json();
    return { google: entry.providers.includes("google"), github: entry.providers.includes("github"),
      email_otp: entry.otp, passkey: entry.passkey };
  } catch { return null; }
}

export function privateJson(body: object, status = 200) {
  return Response.json(body, { status, headers: privateHeaders });
}

export function privateRedirect(url: URL) {
  return new Response(null, { status: 303, headers: { ...privateHeaders, Location: url.href } });
}

export function sameOrigin(request: Request, origin: string) {
  return request.headers.get("origin") === origin;
}

function equal(left: string, right: string) {
  const a = Buffer.from(left);
  const b = Buffer.from(right);
  return a.length === b.length && timingSafeEqual(a, b);
}

export async function signInOptions(request: Request) {
  const capabilities = await getCapabilities(request);
  if (!capabilities) return privateJson({ error: "auth_unavailable" }, 503);
  const last = cookie(request, LAST_USED);
  const lastUsed = last && Object.hasOwn(capabilities, last) && capabilities[last as SignInMethod] ? last : null;
  return Response.json({ capabilities, lastUsed }, { headers: { ...privateHeaders, "Cache-Control": "private, no-store" } });
}

function signInRetry(origin: string, next: string) {
  return privateRedirect(new URL("/sign-in?" + new URLSearchParams({ auth: "failed", next }), origin));
}

export async function startOAuth(request: Request, provider: OAuthProvider) {
  const auth = config();
  if (!auth) return privateJson({ error: "auth_unavailable" }, 503);
  const url = new URL(request.url);
  const next = validateNext(url.searchParams.getAll("next").length === 1 ? url.searchParams.get("next") : null);
  url.pathname = routePrefix + "/" + provider;
  url.search = new URLSearchParams({ returnTo: next }).toString();
  try {
    const response = await sdk(auth)(new Request(url, { headers: request.headers, signal: request.signal }));
    if (response.status === 303 || (response.status === 403 && (await response.clone().json()).error === "not_allowed")) return response;
  } catch { /* Retry through the same-origin sign-in page. */ }
  return signInRetry(auth.origin, next);
}

export async function finishOAuth(request: Request) {
  const auth = config();
  if (!auth) return privateJson({ error: "auth_unavailable" }, 503);
  let signedIn: VerifiedIdentitySession | undefined;
  try {
    const response = await sdk(auth, {
      async issue(session, request) {
        const age = await oauthSessionAge(auth, session, request);
        if (age === null || age < 1 || (session.provider !== "google" && session.provider !== "github")) return;
        signedIn = session;
        return [[SESSION, session.token, age], [LAST_USED, session.provider, lastUsedAge]];
      },
      logout: signOut,
    })(request);
    if (signedIn?.provider && response.status === 303 && response.headers.getSetCookie().some(value => value.startsWith(SESSION + "="))) {
      markSignup(request, response, signedIn.isNewUser, signedIn.provider, signedIn.returnTo);
    }
    return response;
  } catch {
    return privateJson({ error: "auth_unavailable" }, 503);
  }
}

export async function signOut(request: Request) {
  const origin = appOrigin();
  if (!origin) return privateJson({ error: "auth_unavailable" }, 503);
  if (!sameOrigin(request, origin)) return privateJson({ error: "invalid_origin" }, 403);
  const auth = config();
  let upstream: Response | undefined;
  if (auth) {
    const url = new URL(request.url);
    url.pathname = routePrefix + "/logout";
    try { upstream = await sdk(auth)(new Request(url, { method: "POST", headers: request.headers, signal: request.signal })); }
    catch { /* Still clear the local session. */ }
    if (upstream?.status === 403) return upstream;
  }
  const sdkCleared = upstream?.status === 204;
  const noSession = !readAuthSessionCookie(request, { sessionCookie: SESSION });
  const response = privateRedirect(new URL(sdkCleared || noSession ? "/dashboard" : "/dashboard?auth=sign_out_partial", origin));
  if (upstream?.status === 204) {
    for (const value of upstream.headers.getSetCookie()) response.headers.append("Set-Cookie", value);
  } else {
    for (const name of [SESSION, OAUTH]) setPrivateCookie(response, name, "", { maxAge: 0 });
  }
  for (const name of [PASSKEY, EMAIL_FLOW]) setPrivateCookie(response, name, "", { maxAge: 0 });
  clearSignupMarker(request, response);
  return response;
}

// Keep OTP and passkey flow cookies until the Solid SDK can preserve the OTP-step passkey option.
export async function beginPasskey(request: Request) {
  const auth = config();
  if (!auth) return privateJson({ error: "auth_unavailable" }, 503);
  if (!sameOrigin(request, auth.origin)) return privateJson({ error: "invalid_origin" }, 403);
  if (!(await getCapabilities(request, auth))?.passkey) return privateJson({ error: "passkey_unavailable" }, 503);
  const rp = new URL(auth.origin).hostname;
  try {
    const upstream = await providerFetch(clientEndpoint(auth, "sign-in/passkey/begin"), {
      method: "POST", headers: authHeaders(request.headers.get("user-agent"), undefined, true),
      body: JSON.stringify({ rp_id: rp, origin: auth.origin }),
    });
    if (upstream.ok) {
      const body = await upstream.json();
      const { challenge_id, challenge, rp_id, allowed_credential_ids, expires_at } = body;
      const ttl = Math.min(300, Math.floor(expires_at - Date.now() / 1000));
      if (typeof challenge_id === "string" && /^[A-Za-z0-9_-]{1,160}$/.test(challenge_id) &&
        typeof challenge === "string" && challenge.length <= 512 && base64url.test(challenge) &&
        rp_id === rp && Number.isSafeInteger(expires_at) && ttl > 0 && Array.isArray(allowed_credential_ids) &&
        allowed_credential_ids.every((id: unknown) => typeof id === "string" && id.length <= 2048 && base64url.test(id))) {
        const response = privateJson({ challenge_id, challenge, rp_id, allowed_credential_ids });
        setPrivateCookie(response, PASSKEY, challenge_id, { maxAge: ttl });
        return response;
      }
    }
  } catch { /* The provider may not have enabled passkeys for this RP. */ }
  return privateJson({ error: "passkey_unavailable" }, 503);
}

export async function verifyPasskey(request: Request) {
  const auth = config();
  if (!auth) return privateJson({ error: "auth_unavailable" }, 503);
  if (!sameOrigin(request, auth.origin)) return privateJson({ error: "invalid_origin" }, 403);
  if (!(await getCapabilities(request, auth))?.passkey) return privateJson({ error: "passkey_unavailable" }, 503);
  const body = await boundedJson(request, 24_000);
  const challenge = cookie(request, PASSKEY);
  if (!body || !challenge || typeof body.challenge_id !== "string" || !equal(challenge, body.challenge_id)) {
    return privateJson({ error: "invalid_challenge" }, 400);
  }
  const assertion: Record<string, string> = { challenge_id: challenge, session_mode: "server" };
  for (const field of ["credential_id", "client_data_json", "authenticator_data", "signature", "user_handle"]) {
    const value = body[field];
    if (field === "user_handle" && (value === undefined || value === "")) continue;
    if (typeof value !== "string" || value.length > 5500 || !base64url.test(value)) return privateJson({ error: "invalid_assertion" }, 400);
    assertion[field] = value;
  }
  let login = null;
  try {
    const agent = request.headers.get("user-agent");
    const upstream = await providerFetch(clientEndpoint(auth, "sign-in/passkey/verify"), {
      method: "POST", headers: authHeaders(agent, undefined, true), body: JSON.stringify(assertion),
    });
    if (upstream.ok) {
      const { ticket } = await upstream.json();
      if (typeof ticket === "string" && ticket.length <= 2048) login = await redeem(auth, ticket, agent);
    }
  } catch { /* Do not reflect provider response details. */ }
  const next = validateNext(typeof body.next === "string" ? body.next : null);
  const response = login ? privateJson({ next }) : privateJson({ error: "passkey_failed" }, 401);
  setPrivateCookie(response, PASSKEY, "", { maxAge: 0 });
  if (login) {
    setPrivateCookie(response, SESSION, login.token, { maxAge: login.maxAge });
    setPrivateCookie(response, LAST_USED, "passkey", { maxAge: lastUsedAge });
    markSignup(request, response, login.isNewUser, "passkey", next);
    setPrivateCookie(response, EMAIL_FLOW, "", { maxAge: 0 });
  }
  return response;
}

export async function sessionStatus(request: Request) {
  const session = await getSession(request);
  return privateJson({ user: session ? { id: session.id, name: session.name, email: session.email } : null });
}

export async function emailSignIn(request: Request) {
  const auth = config();
  if (!auth) return privateJson({ error: "auth_unavailable" }, 503);
  if (!sameOrigin(request, auth.origin) || !hasPublicOrigin(request, auth.origin)) return privateJson({ error: "invalid_origin" }, 403);
  const body = await boundedJson(request, 8192);
  if (!body) return privateJson({ error: "invalid_input" }, 400);
  const { action, code } = body;
  if (action !== "send" && action !== "verify") return privateJson({ error: "invalid_input" }, 400);
  const email = typeof body.email === "string" ? body.email.trim() : "";
  const next = validateNext(typeof body.next === "string" ? body.next : null);
  const caps = await getCapabilities(request, auth);
  let path: string;
  let input: object;
  if (action === "verify") {
    const flow = cookie(request, EMAIL_FLOW);
    if (!caps?.email_otp || !isCliCode(flow ?? null) || typeof code !== "string" || !/^\d{6}$/.test(code)) {
      return privateJson({ error: "invalid_code" }, 400);
    }
    path = "continue/verify";
    input = { flow, code };
  } else {
    if (!/^[^\s@]+@[^\s@]+$/.test(email) || email.length > 254) return privateJson({ error: "invalid_input" }, 400);
    if (!caps?.email_otp) return privateJson({ error: "method_unavailable" }, 403);
    path = "continue";
    input = { email, method: "email_otp", session_mode: "server" };
  }
  try {
    const agent = request.headers.get("user-agent");
    const upstream = await providerFetch(clientEndpoint(auth, path), {
      method: "POST", headers: authHeaders(agent, undefined, true), body: JSON.stringify(input),
    });
    if (upstream.status === 429) return privateJson({ error: "try_later" }, 429);
    if (!upstream.ok) return privateJson({ error: "auth_failed" }, 401);
    const result = await upstream.json();
    if (action === "send" && result?.step === "email_otp" && isCliCode(result.flow) &&
        Number.isSafeInteger(result.expires_in) && result.expires_in > 0) {
      const response = privateJson({ step: "email_otp", expiresIn: Math.min(result.expires_in, 600) });
      setPrivateCookie(response, EMAIL_FLOW, result.flow, { maxAge: Math.min(result.expires_in, 600) });
      return response;
    }
    if (action === "verify" && typeof result?.ticket === "string" && result.ticket.length <= 2048) {
      const login = await redeem(auth, result.ticket, agent);
      if (login) {
        const response = privateJson({ next });
        setPrivateCookie(response, SESSION, login.token, { maxAge: login.maxAge });
        setPrivateCookie(response, EMAIL_FLOW, "", { maxAge: 0 });
        setPrivateCookie(response, LAST_USED, "email_otp", { maxAge: lastUsedAge });
        markSignup(request, response, login.isNewUser, "email_otp", next);
        return response;
      }
    }
  } catch { /* Never return upstream credentials or errors. */ }
  return privateJson({ error: "auth_unavailable" }, 503);
}

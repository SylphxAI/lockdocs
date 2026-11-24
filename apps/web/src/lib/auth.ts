import { createSylphxAuthHandler, readAuthSessionCookie, type SessionAdapter, type VerifiedIdentitySession } from "@sylphx/auth";
import type { EntryConfig } from "@sylphx/auth/client";
import { timingSafeEqual } from "node:crypto";
import { cookie, setPrivateCookie } from "./http";
import { boundedJson } from "./bounded-json";
import { clearSignupMarker, markSignup } from "./signup-analytics";
import { hasPublicOrigin } from "./public-origin";
import { isCliCode, validateNext } from "./auth-entry";

export type Session = Readonly<{ id: string; name?: string; email?: string; token: string }>;
export type OAuthProvider = "github" | "google";
const methods = ["email_otp", "passkey"] as const;
export type AuthCapabilities = Record<OAuthProvider | typeof methods[number], boolean>;

type AuthConfig = { origin: string; base: string; key: string; secret: string; instance: string };
const SESSION = "__Host-lockdocs-session";
const routePrefix = "/api/auth";
const providers = ["google", "github"] as const;
const OAUTH = "__Host-lockdocs-oauth";
const EMAIL_FLOW = "__Host-lockdocs-email-flow";
const PASSKEY = "__Host-lockdocs-passkey";
const LAST_USED = "__Host-lockdocs-last-auth-method";
const lastUsedAge = 90 * 24 * 60 * 60;
export type SignInMethod = OAuthProvider | typeof methods[number];
const privateHeaders = { "Cache-Control": "no-store", "Referrer-Policy": "no-referrer" };
const verified = new WeakMap<Session, { agent: string | null; expires: number }>();
const base64url = /^[A-Za-z0-9_-]+$/;

export function httpsOrigin(value: string | undefined): string | null {
  if (!value || !/^https:\/\/[^/\\?#@\s%]+\/?$/.test(value)) return null;
  try {
    const url = new URL(value);
    return url.protocol === "https:" && url.hostname && !url.username && !url.password
      ? url.origin
      : null;
  } catch {
    return null;
  }
}

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

export function providerFetch(url: URL, init: Omit<RequestInit, "redirect"> = {}): Promise<Response> {
  return fetch(url, { ...init, cache: "no-store", redirect: "error", signal: AbortSignal.timeout(8_000) });
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

function authHeaders(agent: string | null, credential?: string, json = false) {
  const outgoing = new Headers({ accept: "application/json" });
  if (agent) outgoing.set("user-agent", agent);
  if (credential) outgoing.set("authorization", "Bearer " + credential);
  if (json) outgoing.set("content-type", "application/json");
  return outgoing;
}

function opaqueSession(token: unknown): token is string {
  return typeof token === "string" && token.length < 3_000 && /^identity_org_session_[A-Za-z0-9_-]+$/.test(token);
}

export function endUserId(id: unknown): id is string {
  return typeof id === "string" && id.length <= 160 && /^usr_[A-Za-z0-9_-]+$/.test(id);
}

async function inspect(auth: AuthConfig, token: string, agent: string | null) {
  try {
    const response = await providerFetch(new URL("/v1/client/session", auth.base), {
      headers: authHeaders(agent, token),
    });
    if (!response.ok) return null;
    const body = await response.json();
    const user = body?.user;
    const expiry = body?.session?.expires_at;
    if (body?.session?.instance_id !== auth.instance || typeof body?.session?.id !== "string" ||
      !Number.isSafeInteger(expiry) || expiry <= Date.now() / 1000 || !endUserId(user?.id)) return null;
    const session: Session = Object.freeze({
      id: user.id,
      ...(typeof user.name === "string" && user.name ? { name: user.name } : {}),
      ...(typeof user.email === "string" && user.email ? { email: user.email } : {}),
      token,
    });
    verified.set(session, { agent, expires: expiry });
    return session;
  } catch {
    return null;
  }
}

/** The token is a private Auth session credential: never serialize it to the client. */
export async function getSession(request: Request): Promise<Session | null> {
  const auth = config();
  if (!auth) return null;
  const token = readAuthSessionCookie(request, { sessionCookie: SESSION })?.value;
  if (!opaqueSession(token)) return null;
  const incoming = request.headers;
  return inspect(auth, token, incoming.get("user-agent"));
}

export function isVerifiedSession(session: Session): boolean {
  return (verified.get(session)?.expires ?? 0) > Date.now() / 1000;
}

/** Exchange only a verified direct session; OAuth credentials cannot open Money's end-user routes. */
export async function moneyAccessToken(session: Session): Promise<string | null> {
  const auth = config();
  const context = verified.get(session);
  if (!auth || !context || !isVerifiedSession(session)) return null;
  try {
    const response = await providerFetch(new URL("/v1/client/token", auth.base), {
      method: "POST",
      headers: authHeaders(context.agent, session.token, true),
      body: "{}",
    });
    if (!response.ok) return null;
    const body = await response.json();
    return typeof body?.access_token === "string" &&
      /^[A-Za-z0-9._~-]+$/.test(body.access_token) && body.access_token.length <= 16_384 &&
      Number.isSafeInteger(body.expires_in) && body.expires_in > 0 ? body.access_token : null;
  } catch {
    return null;
  }
}

async function redeem(auth: AuthConfig, ticket: string, agent: string | null) {
  try {
    const response = await providerFetch(new URL("/v1/client/tickets:redeem", auth.base), {
      method: "POST",
      headers: authHeaders(agent, auth.secret, true),
      body: JSON.stringify({ ticket }),
    });
    if (!response.ok) return null;
    const { session, is_new_user } = await response.json();
    if (!opaqueSession(session?.token) || !Number.isSafeInteger(session.expires_at)) return null;
    const identity = await inspect(auth, session.token, agent);
    if (!identity) return null;
    const maxAge = Math.floor(Math.min(session.expires_at, verified.get(identity)!.expires) - Date.now() / 1000);
    return maxAge > 0 ? { token: identity.token, maxAge, isNewUser: is_new_user === true } : null;
  } catch {
    return null;
  }
}

function equal(left: string, right: string) {
  const a = Buffer.from(left);
  const b = Buffer.from(right);
  return a.length === b.length && timingSafeEqual(a, b);
}

// Keep Identity discovery pinned to the instance that backs Lockdocs and Money.
function sdk(auth: AuthConfig, session?: SessionAdapter) {
  return createSylphxAuthHandler({
    origin: auth.origin, authApiOrigin: auth.base, tenantAuthOrigin: process.env.LOCKDOCS_TENANT_AUTH_ORIGIN,
    publishableKey: auth.key, sessionMode: "server", sessionSecret: auth.secret,
    routePrefix, loginPath: "/sign-in", defaultReturnTo: "/dashboard",
    allowReturnTo: path => path === validateNext(path),
    isPublicRequest: request => hasPublicOrigin(request, auth.origin),
    allowedMethods: ["oauth", "otp", "passkey"], providers, directOAuth: true,
    flowCookie: OAUTH, sessionCookie: SESSION, session,
    async fetch(url, init) {
      const response = await fetch(url, init);
      if (url.origin === auth.base && url.pathname === "/v1/client/config" && response.ok) {
        try {
          if ((await response.clone().json()).instance?.id === auth.instance) return response;
        } catch { /* Unreadable discovery must also fail closed. */ }
        void response.body?.cancel().catch(() => undefined);
        return new Response(null, { status: 502 });
      }
      return response;
    },
  });
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

// Keep OTP and passkey flow cookies until the Solid SDK can preserve the OTP-step passkey option.
function clientEndpoint(auth: AuthConfig, path: string) {
  const url = new URL("/v1/client/" + path, auth.base);
  url.searchParams.set("publishable_key", auth.key);
  return url;
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
        if (!opaqueSession(session.token) || typeof session.expiresAt !== "number" || !Number.isSafeInteger(session.expiresAt)) return;
        const identity = await inspect(auth, session.token, request.headers.get("user-agent"));
        if (!identity || identity.id !== session.user.id) return;
        const age = Math.floor(Math.min(session.expiresAt, verified.get(identity)!.expires) - Date.now() / 1000);
        if (age < 1 || (session.provider !== "google" && session.provider !== "github")) return;
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

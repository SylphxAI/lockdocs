import { createSylphxAuthHandler, readAuthSessionCookie, type SessionAdapter, type VerifiedIdentitySession } from "@sylphx/auth";
import { validateNext } from "./auth-entry";

export type Session = Readonly<{ id: string; name?: string; email?: string; token: string }>;
export type OAuthProvider = "github" | "google";
const methods = ["email_otp", "passkey"] as const;
export type AuthCapabilities = Record<OAuthProvider | typeof methods[number], boolean>;

export type AuthConfig = { origin: string; base: string; key: string; secret: string; instance: string };
export const SESSION = "__Host-lockdocs-session";
export const routePrefix = "/api/auth";
const providers = ["google", "github"] as const;
export const OAUTH = "__Host-lockdocs-oauth";
export type SignInMethod = OAuthProvider | typeof methods[number];
const verified = new WeakMap<Session, { agent: string | null; expires: number }>();

export function providerFetch(url: URL, init: Omit<RequestInit, "redirect"> = {}): Promise<Response> {
  return fetch(url, { ...init, cache: "no-store", redirect: "error", signal: AbortSignal.timeout(8_000) });
}

export function authHeaders(agent: string | null, credential?: string, json = false) {
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

export async function inspectRequestSession(request: Request, auth: AuthConfig | null): Promise<Session | null> {
  if (!auth) return null;
  const token = readAuthSessionCookie(request, { sessionCookie: SESSION })?.value;
  if (!opaqueSession(token)) return null;
  const incoming = request.headers;
  return inspect(auth, token, incoming.get("user-agent"));
}

export function isVerifiedSession(session: Session): boolean {
  return (verified.get(session)?.expires ?? 0) > Date.now() / 1000;
}

export async function exchangeMoneyToken(session: Session, auth: AuthConfig | null): Promise<string | null> {
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

export async function redeem(auth: AuthConfig, ticket: string, agent: string | null) {
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
    const maxAge = sessionMaxAge(identity, session.expires_at);
    return maxAge > 0 ? { token: identity.token, maxAge, isNewUser: is_new_user === true } : null;
  } catch {
    return null;
  }
}

function sessionMaxAge(identity: Session, expiresAt: number) {
  return Math.floor(Math.min(expiresAt, verified.get(identity)!.expires) - Date.now() / 1000);
}

export async function oauthSessionAge(auth: AuthConfig, session: VerifiedIdentitySession, request: Request) {
  if (!opaqueSession(session.token) || typeof session.expiresAt !== "number" || !Number.isSafeInteger(session.expiresAt)) return null;
  const identity = await inspect(auth, session.token, request.headers.get("user-agent"));
  if (!identity || identity.id !== session.user.id) return null;
  return sessionMaxAge(identity, session.expiresAt);
}

// Keep Identity discovery pinned to the instance that backs Lockdocs and Money.
export function createIdentityHandler(auth: AuthConfig, tenantAuthOrigin: string | undefined, session?: SessionAdapter) {
  return createSylphxAuthHandler({
    origin: auth.origin, authApiOrigin: auth.base, tenantAuthOrigin,
    publishableKey: auth.key, sessionMode: "server", sessionSecret: auth.secret,
    routePrefix, loginPath: "/sign-in", defaultReturnTo: "/dashboard",
    allowReturnTo: path => path === validateNext(path),
    trustSylphxEdge: true,
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

export function clientEndpoint(auth: AuthConfig, path: string) {
  const url = new URL("/v1/client/" + path, auth.base);
  url.searchParams.set("publishable_key", auth.key);
  return url;
}

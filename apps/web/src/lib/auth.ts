import { cookie, setPrivateCookie } from "./http";
import { boundedJson } from "./bounded-json";
import { randomBytes, timingSafeEqual } from "node:crypto";

export type Session = Readonly<{ id: string; name?: string; email?: string; token: string }>;
export type OAuthProvider = "github" | "google";

type AuthConfig = { origin: string; base: string; key: string; secret: string; instance: string };
const SESSION = "__Host-lockdocs-session";
const OAUTH = "__Host-lockdocs-oauth";
const PASSKEY = "__Host-lockdocs-passkey";
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
  const secret = process.env.SYLPHX_AUTH_SECRET_KEY;
  const instance = process.env.SYLPHX_AUTH_ORGANIZATION_ID;
  return origin && base && key && secret && instance ? { origin, base, key, secret, instance } : null;
}

export function authConfigured(): boolean {
  return config() !== null;
}

export function providerFetch(url: URL, init: RequestInit = {}): Promise<Response> {
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

/** Accept only product-owned pages or the CLI's exact loopback-consent input. */
export function validateNext(value: string | null): string {
  if (value === "/dashboard" || value === "/pricing") return value;
  const prefix = "/cli/authorize?";
  if (!value?.startsWith(prefix) || value.length > 300 || /[^a-zA-Z0-9_?&=/-]/.test(value)) return "/dashboard";
  const query = value.slice(prefix.length);
  if (!/^[a-z]+=[A-Za-z0-9_-]+(?:&[a-z]+=[A-Za-z0-9_-]+){2}$/.test(query)) return "/dashboard";
  const params = new URLSearchParams(query);
  const port = params.get("port");
  const state = params.get("state");
  const challenge = params.get("challenge");
  if (!port || !/^[1-9][0-9]{0,4}$/.test(port) || Number(port) > 65535 ||
      !isCliCode(state) || !isCliCode(challenge)) return "/dashboard";
  return prefix + new URLSearchParams({ port, state, challenge });
}

function isCliCode(value: string | null): value is string {
  return value !== null && /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/.test(value);
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
  const token = cookie(request, SESSION);
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
    const { session } = await response.json();
    if (!opaqueSession(session?.token) || !Number.isSafeInteger(session.expires_at)) return null;
    const identity = await inspect(auth, session.token, agent);
    if (!identity) return null;
    const maxAge = Math.floor(Math.min(session.expires_at, verified.get(identity)!.expires) - Date.now() / 1000);
    return maxAge > 0 ? { token: identity.token, maxAge } : null;
  } catch {
    return null;
  }
}

function equal(left: string, right: string) {
  const a = Buffer.from(left);
  const b = Buffer.from(right);
  return a.length === b.length && timingSafeEqual(a, b);
}

export function startOAuth(request: Request) {
  const auth = config();
  if (!auth) return privateJson({ error: "auth_unavailable" }, 503);
  const params = new URL(request.url).searchParams;
  const providers = params.getAll("provider");
  const provider = providers[0] ?? "github";
  if (providers.length > 1 || (provider !== "github" && provider !== "google")) {
    return privateJson({ error: "invalid_provider" }, 400);
  }
  const state = randomBytes(32).toString("base64url");
  const next = validateNext(params.getAll("next").length === 1 ? params.get("next") : null);
  const callback = new URL("/api/auth/callback", auth.origin);
  callback.searchParams.set("state", state);
  const url = new URL("/v1/client/oauth/" + provider + "/start", auth.base);
  url.search = new URLSearchParams({ publishable_key: auth.key, redirect_url: callback.href, session_mode: "server" }).toString();
  const response = privateRedirect(url);
  setPrivateCookie(response, OAUTH, state + "." + Buffer.from(next).toString("base64url"), { maxAge: 600 });
  return response;
}

function aliased(params: URLSearchParams, canonical: string, legacy: string) {
  const current = params.getAll(canonical);
  const old = params.getAll(legacy);
  if (current.length > 1 || old.length > 1 || (current.length && old.length && current[0] !== old[0])) return null;
  return current[0] ?? old[0] ?? "";
}

export async function finishOAuth(request: Request) {
  const auth = config();
  if (!auth) return privateJson({ error: "auth_unavailable" }, 503);
  const [state, encodedNext] = (cookie(request, OAUTH) ?? "").split(".");
  const params = new URL(request.url).searchParams;
  const states = params.getAll("state");
  if (!isCliCode(state) || states.length !== 1 || !equal(state, states[0])) {
    return privateJson({ error: "invalid_oauth_state" }, 400);
  }
  const next = validateNext(encodedNext ? Buffer.from(encodedNext, "base64url").toString("utf8") : null);
  const ticket = aliased(params, "auth_ticket", "sylphx_ticket");
  const error = aliased(params, "auth_error", "sylphx_error");
  const login = error === "" && ticket && ticket.length <= 2048
    ? await redeem(auth, ticket, request.headers.get("user-agent")) : null;
  const response = privateRedirect(new URL(login ? next : "/dashboard?auth=failed", auth.origin));
  setPrivateCookie(response, OAUTH, "", { maxAge: 0 });
  if (login) setPrivateCookie(response, SESSION, login.token, { maxAge: login.maxAge });
  return response;
}

export async function signOut(request: Request) {
  const origin = appOrigin();
  if (!origin) return privateJson({ error: "auth_unavailable" }, 503);
  if (!sameOrigin(request, origin)) return privateJson({ error: "invalid_origin" }, 403);
  const auth = config();
  const token = cookie(request, SESSION);
  let revoked = !opaqueSession(token);
  if (auth && opaqueSession(token)) {
    try {
      const result = await providerFetch(new URL("/v1/client/sign-out", auth.base), {
        method: "POST",
        headers: authHeaders(request.headers.get("user-agent"), token),
      });
      revoked = result.ok || result.status === 401;
    } catch { /* Still clear the local session. */ }
  }
  const response = privateRedirect(new URL(revoked ? "/dashboard" : "/dashboard?auth=sign_out_partial", origin));
  for (const name of [SESSION, OAUTH, PASSKEY]) setPrivateCookie(response, name, "", { maxAge: 0 });
  return response;
}

function passkeyEndpoint(auth: AuthConfig, phase: "begin" | "verify") {
  const url = new URL("/v1/client/sign-in/passkey/" + phase, auth.base);
  url.searchParams.set("publishable_key", auth.key);
  return url;
}

export async function beginPasskey(request: Request) {
  const auth = config();
  if (!auth) return privateJson({ error: "auth_unavailable" }, 503);
  if (!sameOrigin(request, auth.origin)) return privateJson({ error: "invalid_origin" }, 403);
  const rp = new URL(auth.origin).hostname;
  try {
    const upstream = await providerFetch(passkeyEndpoint(auth, "begin"), {
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
    const upstream = await providerFetch(passkeyEndpoint(auth, "verify"), {
      method: "POST", headers: authHeaders(agent, undefined, true), body: JSON.stringify(assertion),
    });
    if (upstream.ok) {
      const { ticket } = await upstream.json();
      if (typeof ticket === "string" && ticket.length <= 2048) login = await redeem(auth, ticket, agent);
    }
  } catch { /* Do not reflect provider response details. */ }
  const response = login ? privateJson({ next: validateNext(typeof body.next === "string" ? body.next : null) }) : privateJson({ error: "passkey_failed" }, 401);
  setPrivateCookie(response, PASSKEY, "", { maxAge: 0 });
  if (login) setPrivateCookie(response, SESSION, login.token, { maxAge: login.maxAge });
  return response;
}

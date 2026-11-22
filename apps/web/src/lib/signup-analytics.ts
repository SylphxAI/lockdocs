import { cookie } from "./http";
import { sendsGpc } from "./analytics-policy";
import { measurementId } from "./analytics-config";

const grantCookie = "__Host-lockdocs-signup-consent";
const markerCookie = "__Host-lockdocs-signup";
const methods = ["google", "github", "email_otp", "passkey"] as const;
const surfaces = ["dashboard", "pricing", "cli"] as const;

function optionalCookie(name: string, value: string, maxAge: number) {
  return name + "=" + value + "; Max-Age=" + maxAge + "; Path=/; Secure; SameSite=Lax";
}

export function clearSignupMarker(request: Request, response: Response) {
  if (cookie(request, markerCookie) !== undefined) {
    response.headers.append("Set-Cookie", optionalCookie(markerCookie, "", 0));
  }
}

/** Call only after a verified login. Never place session or request data in the marker. */
export function markSignup(request: Request, response: Response, isNewUser: boolean, method: string, next: string) {
  const expires = Number(cookie(request, grantCookie));
  if (isNewUser !== true || !methods.some(value => value === method) || !measurementId() ||
      sendsGpc(request) || !Number.isSafeInteger(expires) || expires <= Date.now()) {
    clearSignupMarker(request, response);
    return;
  }
  const path = next.split("?", 1)[0];
  const surface = path === "/pricing" ? "pricing" : path === "/cli/authorize" ? "cli" : "dashboard";
  response.headers.append("Set-Cookie", optionalCookie(markerCookie, method + "." + surface, 60));
}

function readBrowserCookie(name: string) {
  return document.cookie.split(";").map(part => part.trim()).find(part => part.startsWith(name + "="))?.slice(name.length + 1);
}

/** Mirror only a saved grant so the server can avoid setting a marker on opt-out. */
export function syncSignupConsent(expires: number | null) {
  const current = readBrowserCookie(grantCookie);
  const lifetime = expires === null ? 0 : Math.floor((expires - Date.now()) / 1000);
  if (lifetime <= 0) {
    if (current !== undefined) document.cookie = optionalCookie(grantCookie, "", 0);
    if (readBrowserCookie(markerCookie) !== undefined) document.cookie = optionalCookie(markerCookie, "", 0);
  } else if (current !== String(expires)) {
    document.cookie = optionalCookie(grantCookie, String(expires), lifetime);
  }
}

export async function consumeSignup(allowed: () => boolean, emit: (params: { method: string; entry_surface: string }) => void) {
  if (!allowed() || !readBrowserCookie(markerCookie) || !navigator.locks) return;
  // Shared cookies can be seen by two tabs; serialize the read/clear/emit.
  await navigator.locks.request(markerCookie, () => {
    if (!allowed()) return;
    const marker = readBrowserCookie(markerCookie);
    if (!marker) return;
    document.cookie = optionalCookie(markerCookie, "", 0);
    if (readBrowserCookie(markerCookie) !== undefined) return;
    const [method, entry_surface, extra] = marker.split(".");
    if (extra === undefined && methods.some(value => value === method) && surfaces.some(value => value === entry_surface)) {
      emit({ method, entry_surface });
    }
  });
}

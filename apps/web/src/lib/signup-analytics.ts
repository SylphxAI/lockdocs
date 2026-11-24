import { cookie } from "./http";
import { resolveEffectiveConsent } from "@sylphx/sdk/signal";
import { signalRequestPrivacy } from "./signal-privacy";
import { measurementId } from "./analytics-config";

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
  if (isNewUser !== true || !methods.some(value => value === method) || !measurementId() ||
      resolveEffectiveConsent(signalRequestPrivacy(request), request.headers).analytics_storage !== "granted") {
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

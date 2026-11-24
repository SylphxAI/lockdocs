import { resolveEffectiveConsent, type SignalPrivacy } from "@sylphx/sdk/signal";

export const signalStorageKey = "lockdocs.signal.v1";
export const signalChoiceKey = signalStorageKey + ".consent";
export const signalChoiceCookie = "__Host-lockdocs-signal-choice";
export const choiceLifetime = 180 * 24 * 60 * 60;
export type StoredChoice = NonNullable<SignalPrivacy["storedChoice"]>;

export function readSignalChoice(cookies: string): StoredChoice | undefined {
  const prefix = signalChoiceCookie + "=";
  const [entry, ...duplicates] = cookies.split(";").map(part => part.trim()).filter(part => part.startsWith(prefix));
  if (entry === undefined) return;
  const value = entry.slice(prefix.length);
  return !duplicates.length && (value === "granted" || value === "analytics_only") ? value : "denied";
}

export function signalRequestGpc(headers: Headers) {
  return resolveEffectiveConsent({ storedChoice: "granted" }, headers).analytics_storage === "denied";
}

export function signalRequestPrivacy(request: Request): SignalPrivacy {
  // Cloudflare sets the country on the request at the public edge. Do not
  // substitute a browser-provided country or geolocate the server's IP.
  return { region: request.headers.get("cf-ipcountry") ?? undefined,
    storedChoice: readSignalChoice(request.headers.get("cookie") ?? "") };
}

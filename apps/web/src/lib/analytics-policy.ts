export type Choice = "reject" | "analytics" | "all";
export const preferenceKey = "lockdocs.cookie-settings.v1";
export const preferenceLifetime = 180 * 24 * 60 * 60 * 1000;

export function savedPreference(raw: string | null): { choice: Choice; expires: number } | null {
  if (!raw) return null;
  try {
    const { choice, expires } = JSON.parse(raw);
    const now = Date.now();
    return (choice === "reject" || choice === "analytics" || choice === "all") &&
      Number.isSafeInteger(expires) && expires > now && expires <= now + preferenceLifetime ? { choice, expires } : null;
  } catch { return null; }
}

export function sendsGpc(request: Request) {
  return request.headers.get("sec-gpc")?.split(",").some(value => value.trim() === "1") ?? false;
}

export function consent(choice: Choice) {
  return {
    analytics_storage: choice === "reject" ? "denied" : "granted",
    ad_storage: choice === "all" ? "granted" : "denied",
    ad_user_data: choice === "all" ? "granted" : "denied",
    ad_personalization: "denied",
  } as const;
}

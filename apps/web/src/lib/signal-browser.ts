import { choiceLifetime, readSignalChoice, signalChoiceCookie, type StoredChoice } from "./signal-privacy";

export function saveSignalChoice(choice: StoredChoice, seconds = choiceLifetime) {
  document.cookie = `${signalChoiceCookie}=${choice}; Max-Age=${seconds}; Path=/; Secure; SameSite=Lax`;
  return readSignalChoice(document.cookie) === choice;
}

export function migrateSignalChoice() {
  const key = "lockdocs.cookie-settings.v1";
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return;
    if (readSignalChoice(document.cookie) !== undefined) {
      localStorage.removeItem(key);
      return;
    }
    const { choice, expires } = JSON.parse(raw);
    const seconds = Math.floor((expires - Date.now()) / 1000);
    if (Number.isSafeInteger(expires) && seconds > 0 && seconds <= choiceLifetime) {
      // An old "all" grant never covered ad personalization.
      const migrated = choice === "analytics" || choice === "all" ? "analytics_only" : "denied";
      if (!saveSignalChoice(migrated, seconds)) return "denied";
    }
    localStorage.removeItem(key);
  } catch { return "denied"; }
}

export function clearAnalyticsCookies() {
  for (const name of document.cookie.split(";").map(entry => entry.trim().split("=", 1)[0])) {
    if (/^(_ga(?:_|$)|_gid$|_gat(?:_|$)|_gcl_|_gac_|_fb[cp]$)/.test(name)) {
      document.cookie = name + "=; Max-Age=0; Path=/; SameSite=Lax; Secure";
      document.cookie = name + "=; Max-Age=0; Path=/; SameSite=Lax; Secure; Domain=" + location.hostname;
    }
  }
}

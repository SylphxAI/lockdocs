import { traceAllowsDefault } from "./analytics-policy";

export function gpc() {
  return (navigator as Navigator & { globalPrivacyControl?: boolean }).globalPrivacyControl === true;
}

export async function allowsDefault() {
  try {
    const response = await fetch("/cdn-cgi/trace", {
      credentials: "omit", cache: "no-store", mode: "same-origin", redirect: "error",
      referrerPolicy: "no-referrer", signal: AbortSignal.timeout(2000),
    });
    return response.ok && response.headers.get("content-type")?.split(";")[0].trim() === "text/plain" &&
      traceAllowsDefault(await response.text());
  } catch { return false; }
}

export function clearAnalyticsCookies() {
  for (const name of document.cookie.split(";").map(entry => entry.trim().split("=", 1)[0])) {
    if (/^(_ga(?:_|$)|_gid$|_gat(?:_|$)|_gcl_|_gac_)/.test(name)) {
      document.cookie = name + "=; Max-Age=0; Path=/; SameSite=Lax";
      document.cookie = name + "=; Max-Age=0; Path=/; SameSite=Lax; Domain=" + location.hostname;
    }
  }
}

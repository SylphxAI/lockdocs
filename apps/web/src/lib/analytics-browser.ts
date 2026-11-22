export function gpc() {
  return (navigator as Navigator & { globalPrivacyControl?: boolean }).globalPrivacyControl === true;
}

export function clearAnalyticsCookies() {
  for (const name of document.cookie.split(";").map(entry => entry.trim().split("=", 1)[0])) {
    if (/^(_ga(?:_|$)|_gid$|_gat(?:_|$)|_gcl_|_gac_)/.test(name)) {
      document.cookie = name + "=; Max-Age=0; Path=/; SameSite=Lax";
      document.cookie = name + "=; Max-Age=0; Path=/; SameSite=Lax; Domain=" + location.hostname;
    }
  }
}

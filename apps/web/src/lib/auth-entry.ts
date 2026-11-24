const aliases = new Set(["/login", "/signup", "/sign-up", "/register", "/forgot-password"]);

export function signInHref(next: string) {
  return "/sign-in?" + new URLSearchParams({ next });
}

export function pricingPath(currency: string) {
  return currency === "USD" ? "/pricing" : "/pricing?" + new URLSearchParams({ currency });
}

export function isCliCode(value: string | null): value is string {
  return value !== null && /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/.test(value);
}

/** Accept only known return pages and the CLI's exact loopback-consent input. */
export function validateNext(value: string | null): string {
  if (value && (value === "/dashboard" || /^(?:\/zh-TW)?\/pricing(?:\?currency=(?:USD|GBP|EUR|HKD))?$/.test(value))) {
    return value;
  }
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

export function legacySignIn(request: Request) {
  if (request.method !== "GET" && request.method !== "HEAD") return;
  const url = new URL(request.url);
  if (!aliases.has(url.pathname.replace(/\/$/, ""))) return;
  const candidates = ["next", "nextUrl", "returnUrl", "returnTo"].flatMap(key => url.searchParams.getAll(key));
  const location = candidates.length ? signInHref(validateNext(candidates.length === 1 ? candidates[0] : null)) : "/sign-in";
  return new Response(null, { status: 308, headers: { Location: location, "Cache-Control": "private, no-store" } });
}

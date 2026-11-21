export const canonicalOrigin = "https://lockdocs.sylphx.com";
const live = new URL(canonicalOrigin).hostname;
function hostname(authority: string | null) {
  if (!authority || !/^[a-zA-Z0-9.-]+(?::[0-9]{1,5})?$/.test(authority)) return null;
  return authority.split(":")[0].toLowerCase();
}
// Fail closed if either authority is unknown, ambiguous or a preview hostname.
export function allowIndexing(headers: Headers) {
  const host = hostname(headers.get("host"));
  const forwarded = headers.get("x-forwarded-host");
  return host === live && (forwarded === null || hostname(forwarded) === live);
}

import type { APIEvent } from "@solidjs/start/server";
import { appOrigin, getSession, privateJson, privateRedirect, sameOrigin } from "@/lib/auth";
import { readConsent, sealGrant } from "@/lib/cli-envelope";
import { getEntitlement, listMine } from "@/lib/money";

export async function POST({ request }: APIEvent) {
  const origin = appOrigin();
  const secret = process.env.SYLPHX_AUTH_SECRET_KEY;
  if (!origin || !secret) return privateJson({ error: "cli_unavailable" }, 503);
  if (!sameOrigin(request, origin)) return privateJson({ error: "invalid_origin" }, 403);
  const session = await getSession(request);
  if (!session) return privateRedirect(new URL("/dashboard?auth=required", origin));
  if (request.headers.get("content-type")?.split(";", 1)[0] !== "application/x-www-form-urlencoded") {
    return privateJson({ error: "invalid_consent" }, 400);
  }
  let consent;
  try {
    const form = await request.formData();
    if ([...form].length !== 3) return privateJson({ error: "invalid_consent" }, 400);
    consent = readConsent(Object.fromEntries(form));
  } catch {
    return privateJson({ error: "invalid_consent" }, 400);
  }
  if (!consent) return privateJson({ error: "invalid_consent" }, 400);
  const entitlement = await getEntitlement(session);
  if (!entitlement.entitled) return privateRedirect(new URL("/dashboard?cli=" + (entitlement.status === "not_entitled" ? "no_pro" : "unavailable"), origin));
  let pageToken = "";
  // The provider owns the subject filter. Do not fall back to a project-level read.
  for (let page = 0; page < 3; page++) {
    const result = await listMine(session, { pageToken });
    if (result.status !== "ok") break;
    const licence = result.licence_tokens.sort((a, b) => Date.parse(b.expire_time) - Date.parse(a.expire_time))[0];
    if (licence?.token && licence.token.length <= 4000) {
      const location = new URL("http://127.0.0.1:" + consent.port + "/callback");
      location.search = new URLSearchParams({ state: consent.state, code: sealGrant(secret, origin, consent, licence.token) }).toString();
      return privateRedirect(location);
    }
    if (!result.next_page_token || result.next_page_token === pageToken) break;
    pageToken = result.next_page_token;
  }
  return privateRedirect(new URL("/dashboard?cli=unavailable", origin));
}

import type { APIEvent } from "@solidjs/start/server";
import { appOrigin, emailSignIn, privateJson, privateRedirect, startOAuth } from "@/lib/auth";
import { signInHref, validateNext } from "@/lib/auth-entry";
import { hasPublicOrigin } from "@/lib/public-origin";

export const POST = ({ request }: APIEvent) => emailSignIn(request);
export function GET({ request }: APIEvent) {
  const params = new URL(request.url).searchParams;
  const providers = params.getAll("provider");
  if (providers.length === 1 && (providers[0] === "github" || providers[0] === "google")) return startOAuth(request, providers[0]);
  if (providers.length) return privateJson({ error: "invalid_provider" }, 400);
  // Old checkout/portal sign-in links enter the unified screen without changing billing handlers.
  const origin = appOrigin();
  if (!origin || !hasPublicOrigin(request, origin)) return privateJson({ error: "invalid_origin" }, 403);
  const next = validateNext(params.getAll("next").length === 1 ? params.get("next") : null);
  return privateRedirect(new URL(signInHref(next), origin));
}

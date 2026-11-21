import type { APIEvent } from "@solidjs/start/server";
import { appOrigin, privateJson } from "@/lib/auth";
import { boundedJson } from "@/lib/bounded-json";
import { openGrant } from "@/lib/cli-envelope";

export async function POST({ request }: APIEvent) {
  const origin = appOrigin();
  const secret = process.env.SYLPHX_AUTH_SECRET_KEY;
  if (!origin || !secret) return privateJson({ error: "cli_unavailable" }, 503);
  // The native client does not send Origin. Browser submissions are never needed here.
  if (request.headers.has("origin")) return privateJson({ error: "invalid_origin" }, 403);
  const input = await boundedJson(request, 17_000);
  const grant = input && openGrant(secret, origin, input);
  return grant ? privateJson({ grant }) : privateJson({ error: "invalid_exchange" }, 400);
}

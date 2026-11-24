import type { APIEvent } from "@solidjs/start/server";
import { privateJson, startOAuth } from "@/lib/auth";

export function GET({ request, params }: APIEvent) {
  const { provider } = params;
  return provider === "github" || provider === "google"
    ? startOAuth(request, provider)
    : privateJson({ error: "not_found" }, 404);
}

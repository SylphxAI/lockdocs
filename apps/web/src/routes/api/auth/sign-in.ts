import type { APIEvent } from "@solidjs/start/server";
import { emailSignIn, privateJson, startOAuth } from "@/lib/auth";

export const POST = ({ request }: APIEvent) => emailSignIn(request);
export function GET({ request }: APIEvent) {
  const values = new URL(request.url).searchParams.getAll("provider");
  const provider = values[0] ?? "github";
  return values.length <= 1 && (provider === "github" || provider === "google")
    ? startOAuth(request, provider) : privateJson({ error: "invalid_provider" }, 400);
}

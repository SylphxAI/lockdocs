import type { APIEvent } from "@solidjs/start/server";
import { measurementId } from "@/lib/analytics-config";
import { signalRequestGpc, signalRequestPrivacy } from "@/lib/signal-privacy";

export const GET = ({ request }: APIEvent) => Response.json({
  measurementId: measurementId(),
  region: signalRequestPrivacy(request).region ?? null,
  gpc: signalRequestGpc(request.headers),
}, { headers: { "Cache-Control": "private, no-store" } });

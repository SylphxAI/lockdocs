import type { APIEvent } from "@solidjs/start/server";
import { analyticsFramePolicy } from "@/lib/content-security-policy";
import { measurementId } from "@/lib/analytics-config";
import { signalRequestGpc } from "@/lib/signal-privacy";

// A script-free, same-origin document for the Google tag. The parent rewrites
// its URL to a sanitized storefront page before it can load any Google code.
export const GET = ({ request }: APIEvent) => {
  const headers = { "Cache-Control": "no-store", "Referrer-Policy": "no-referrer" };
  if (!measurementId() || signalRequestGpc(request.headers)) return new Response(null, { status: 404, headers });
  return new Response('<!doctype html><html data-analytics-frame lang="en"><meta charset="utf-8"><title>Lockdocs analytics</title></html>', {
    headers: { ...headers, "Content-Type": "text/html; charset=utf-8", "X-Frame-Options": "SAMEORIGIN",
      "Content-Security-Policy": analyticsFramePolicy },
  });
};

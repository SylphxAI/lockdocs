import { measurementId } from "@/lib/analytics-config";

export const GET = () => Response.json({ measurementId: measurementId() }, {
  headers: { "Cache-Control": "no-store" },
});

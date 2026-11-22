import type { APIEvent } from "@solidjs/start/server";
import { verifiedPurchase } from "@/lib/money";

export const POST = ({ request }: APIEvent) => verifiedPurchase(request);

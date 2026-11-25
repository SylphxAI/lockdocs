import type { APIEvent } from "@solidjs/start/server";
import { claimPurchaseConversion } from "@/lib/purchase-conversion";

export const POST = ({ request }: APIEvent) => claimPurchaseConversion(request);

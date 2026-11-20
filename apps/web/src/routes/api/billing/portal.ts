import type { APIEvent } from "@solidjs/start/server";
import { portal } from "@/lib/money";

export const POST = ({ request }: APIEvent) => portal(request);

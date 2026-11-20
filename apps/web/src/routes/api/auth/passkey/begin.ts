import type { APIEvent } from "@solidjs/start/server";
import { beginPasskey } from "@/lib/auth";
export const POST = ({ request }: APIEvent) => beginPasskey(request);

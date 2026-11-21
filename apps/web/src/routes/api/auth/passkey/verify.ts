import type { APIEvent } from "@solidjs/start/server";
import { verifyPasskey } from "@/lib/auth";
export const POST = ({ request }: APIEvent) => verifyPasskey(request);

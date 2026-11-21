import type { APIEvent } from "@solidjs/start/server";
import { sessionStatus } from "@/lib/auth";
export const GET = ({ request }: APIEvent) => sessionStatus(request);

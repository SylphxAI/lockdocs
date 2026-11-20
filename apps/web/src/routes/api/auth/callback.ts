import type { APIEvent } from "@solidjs/start/server";
import { finishOAuth } from "@/lib/auth";

export const GET = ({ request }: APIEvent) => finishOAuth(request);

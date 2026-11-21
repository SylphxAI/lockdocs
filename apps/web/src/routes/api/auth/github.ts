import type { APIEvent } from "@solidjs/start/server";
import { startOAuth } from "@/lib/auth";
export const GET = ({ request }: APIEvent) => startOAuth(request, "github");

import type { APIEvent } from "@solidjs/start/server";
import { signInOptions } from "@/lib/auth";

export const GET = ({ request }: APIEvent) => signInOptions(request);

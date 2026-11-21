import type { APIEvent } from "@solidjs/start/server";
import { signOut } from "@/lib/auth";

export const POST = ({ request }: APIEvent) => signOut(request);

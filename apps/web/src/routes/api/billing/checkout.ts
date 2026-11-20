import type { APIEvent } from "@solidjs/start/server";
import { checkout } from "@/lib/money";

export const POST = ({ request }: APIEvent) => checkout(request);

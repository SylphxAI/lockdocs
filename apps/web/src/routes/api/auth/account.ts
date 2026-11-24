import type { APIEvent } from "@solidjs/start/server";
import { accountDetails } from "@/lib/account";
import { privateJson } from "@/lib/auth";

export const GET = async ({ request }: APIEvent) => privateJson(await accountDetails(request));

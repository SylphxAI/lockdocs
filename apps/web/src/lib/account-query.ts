import { query } from "@solidjs/router";
import { getRequestEvent } from "solid-js/web";
import { getSession } from "./auth";
import { billingConfigured, getEntitlement } from "./money";

export const account = query(async () => {
  "use server";
  const event = getRequestEvent();
  if (!event) throw new Error("Missing request");
  event.response.headers.set("Cache-Control", "private, no-store");
  const session = await getSession(event.request);
  const entitlement = await getEntitlement(session);
  return { user: session ? { name: session.name || session.email || "Signed in" } : null, entitlement, billing: billingConfigured() };
}, "account");

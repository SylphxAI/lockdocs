import { query } from "@solidjs/router";
import { getRequestEvent } from "solid-js/web";
import { getSession, validateNext } from "./auth";
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

export const signInDetails = query(async (next: string | null) => {
  "use server";
  const event = getRequestEvent();
  if (!event) throw new Error("Missing request");
  event.response.headers.set("Cache-Control", "private, no-store");
  const session = await getSession(event.request);
  return { signedIn: Boolean(session), next: validateNext(next) };
}, "sign-in-details");

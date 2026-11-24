import { query } from "@solidjs/router";
import { getRequestEvent } from "solid-js/web";
import type { accountDetails } from "./account";

export const account = query(async (): ReturnType<typeof accountDetails> => {
  if (import.meta.env.SSR) {
    const event = getRequestEvent();
    if (!event) throw new Error("Missing request");
    event.response.headers.set("Cache-Control", "private, no-store");
    return (await import("./account")).accountDetails(event.request);
  }
  // SolidStart's server-function transport evaluates serialized JavaScript on
  // client navigation. Account data fits JSON and doesn't need script evaluation.
  const response = await fetch("/api/auth/account", { cache: "no-store" });
  if (!response.ok) throw new Error("Account status unavailable");
  return response.json();
}, "account");

import { query } from "@solidjs/router";
import { getRequestEvent } from "solid-js/web";
import type { SignInOptions } from "./auth";

export type { SignInOptions } from "./auth";

/** Sign-in methods to draw. Null means they couldn't be loaded; the form offers a retry. */
export const signInOptions = query(async (): Promise<SignInOptions | null> => {
  if (import.meta.env.SSR) {
    const event = getRequestEvent();
    if (!event) throw new Error("Missing request");
    event.response.headers.set("Cache-Control", "private, no-store");
    return (await import("./auth")).pageSignInOptions(event.request);
  }
  // Plain JSON over the existing endpoint, as for account status: no server-function transport.
  try {
    const response = await fetch("/api/auth/options", { cache: "no-store" });
    return response.ok ? await response.json() : null;
  } catch { return null; }
}, "sign-in-options");

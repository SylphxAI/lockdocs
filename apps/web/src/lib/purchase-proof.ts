import { createHmac, randomUUID, timingSafeEqual } from "node:crypto";
import { cookie, setPrivateCookie } from "./http";

const name = "__Host-lockdocs-analytics-checkout";
const maxAge = 24 * 60 * 60;
const checkoutId = /^[A-Za-z0-9_-]{1,128}$/;
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const sign = (secret: string, subject: string, body: string) => createHmac("sha256", secret)
  .update(JSON.stringify([name, subject, body])).digest();

export function rememberCheckout(response: Response, secret: string, subject: string, id: string) {
  if (!checkoutId.test(id)) return false;
  // An independent analytics deduplication key; never send a Money/Stripe ID to Google.
  const body = [id, randomUUID(), Math.floor(Date.now() / 1000) + maxAge].join(".");
  setPrivateCookie(response, name, body + "." + sign(secret, subject, body).toString("base64url"), { maxAge });
  return true;
}

export function pendingCheckout(request: Request, secret: string, subject: string) {
  const parts = cookie(request, name)?.split(".", 5);
  if (parts?.length !== 4) return null;
  const [id, transactionId, expiry, signature] = parts;
  if (!checkoutId.test(id) || !uuid.test(transactionId) || !/^\d{10}$/.test(expiry) ||
      Number(expiry) <= Date.now() / 1000 || !/^[A-Za-z0-9_-]{43}$/.test(signature)) return null;
  const digest = Buffer.from(signature, "base64url");
  if (!timingSafeEqual(digest, sign(secret, subject, parts.slice(0, 3).join(".")))) return null;
  return { id, transactionId };
}

export function forgetCheckout(response: Response) {
  setPrivateCookie(response, name, "", { maxAge: 0 });
}

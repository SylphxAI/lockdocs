import { createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import { cookie, setPrivateCookie } from "./http";
import { isPurchaseReference } from "./analytics-events";

const name = "__Host-lockdocs-analytics-checkout";
const maxAge = 24 * 60 * 60;
const checkoutId = /^[A-Za-z0-9_-]{1,128}$/;
const sign = (secret: string, subject: string, body: string) => createHmac("sha256", secret)
  .update(JSON.stringify([name, subject, body])).digest();

export function rememberCheckout(response: Response, secret: string, subject: string, id: string) {
  if (!checkoutId.test(id)) return false;
  // An opaque random GA4 deduplication token, not an entity ID or a checkout identifier.
  const body = [id, randomBytes(20).toString("hex"), Math.floor(Date.now() / 1000) + maxAge].join(".");
  setPrivateCookie(response, name, body + "." + sign(secret, subject, body).toString("base64url"), { maxAge });
  return true;
}

export function pendingCheckout(request: Request, secret: string, subject: string) {
  const parts = cookie(request, name)?.split(".", 5);
  if (parts?.length !== 4) return null;
  const [id, transactionId, expiry, signature] = parts;
  if (!checkoutId.test(id) || !isPurchaseReference(transactionId) || !/^\d{10}$/.test(expiry) ||
      Number(expiry) <= Date.now() / 1000 || !/^[A-Za-z0-9_-]{43}$/.test(signature)) return null;
  const digest = Buffer.from(signature, "base64url");
  if (!timingSafeEqual(digest, sign(secret, subject, parts.slice(0, 3).join(".")))) return null;
  return { id, transactionId };
}

export function forgetCheckout(response: Response) {
  setPrivateCookie(response, name, "", { maxAge: 0 });
}

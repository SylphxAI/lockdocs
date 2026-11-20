import { isVerifier, type Consent } from "./cli-consent";
import { createCipheriv, createDecipheriv, createHash, hkdfSync, randomBytes, timingSafeEqual } from "node:crypto";

const nonceLength = 12;
const tagLength = 16;
const lifetime = 90_000;
const context = Buffer.from("lockdocs-cli-v1");

function key(secret: string, audience: string) {
  return Buffer.from(hkdfSync("sha256", secret, context, audience, 32));
}

export function sealGrant(secret: string, audience: string, { state, challenge }: Consent, grant: string, now = Date.now()) {
  const nonce = randomBytes(nonceLength);
  const cipher = createCipheriv("aes-256-gcm", key(secret, audience), nonce, { authTagLength: tagLength });
  cipher.setAAD(context);
  const payload = JSON.stringify({ state, challenge, grant, exp: now + lifetime });
  return Buffer.concat([nonce, cipher.update(payload, "utf8"), cipher.final(), cipher.getAuthTag()]).toString("base64url");
}

export function openGrant(secret: string, audience: string, input: Record<string, unknown>, now = Date.now()) {
  const { code, state, verifier } = input;
  if (typeof code !== "string" || code.length > 16_000 || !/^[A-Za-z0-9_-]+$/.test(code) ||
      !isVerifier(state) || !isVerifier(verifier)) return null;
  try {
    const data = Buffer.from(code, "base64url");
    if (data.length <= nonceLength + tagLength || data.toString("base64url") !== code) return null;
    const decipher = createDecipheriv("aes-256-gcm", key(secret, audience), data.subarray(0, nonceLength), { authTagLength: tagLength });
    decipher.setAAD(context);
    decipher.setAuthTag(data.subarray(-tagLength));
    const payload = JSON.parse(Buffer.concat([decipher.update(data.subarray(nonceLength, -tagLength)), decipher.final()]).toString("utf8"));
    const digest = createHash("sha256").update(Buffer.from(verifier, "base64url")).digest();
    if (!isVerifier(payload.state) || !isVerifier(payload.challenge) ||
      !timingSafeEqual(Buffer.from(payload.state, "base64url"), Buffer.from(state, "base64url")) ||
      !timingSafeEqual(Buffer.from(payload.challenge, "base64url"), digest) ||
      !Number.isSafeInteger(payload.exp) || payload.exp <= now || payload.exp > now + lifetime ||
      typeof payload.grant !== "string" || !payload.grant || payload.grant.length > 4000) return null;
    return payload.grant as string;
  } catch {
    return null;
  }
}

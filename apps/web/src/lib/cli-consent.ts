export type Consent = Readonly<{ port: number; state: string; challenge: string }>;

export function isVerifier(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/.test(value);
}

export function readConsent(values: Record<string, unknown>): Consent | null {
  const { port, state, challenge } = values;
  if (typeof port !== "string" || !/^[1-9][0-9]{0,4}$/.test(port) || Number(port) > 65535 ||
      !isVerifier(state) || !isVerifier(challenge)) return null;
  return { port: Number(port), state, challenge };
}

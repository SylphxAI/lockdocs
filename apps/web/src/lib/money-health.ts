import { httpsOrigin } from "./http";

type ProbeResult = "ok" | "error";
type Config = { base: string; parent: string; secret: string };
const environmentPath = /^orgs\/[A-Za-z0-9_-]+\/projects\/[A-Za-z0-9_-]+\/envs\/[A-Za-z0-9_-]+$/;

let cached: { config: Config; result: Promise<ProbeResult>; expiresAt: number } | undefined;

async function probe({ base, parent, secret }: Config): Promise<ProbeResult> {
  try {
    // Use one deadline for fetch and body consumption; never forward the key on redirects.
    const signal = AbortSignal.timeout(2_000);
    const response = await fetch(new URL("/v1/whoami", base), {
      method: "GET",
      headers: { authorization: "Bearer " + secret, accept: "application/json" },
      cache: "no-store",
      redirect: "error",
      signal,
    });
    if (!response.ok) {
      void response.body?.cancel().catch(() => {});
      return "error";
    }
    const body = await response.json();
    return typeof body?.env === "string" && body.env.trim() === parent ? "ok" : "error";
  } catch {
    return "error";
  }
}

/** A read-only diagnostic for the deployed service key, independent of callers and checkout. */
export function moneyHealth(): Promise<ProbeResult | "unconfigured"> {
  const base = httpsOrigin(process.env.SYLPHX_MONEY_URL);
  const parent = process.env.SYLPHX_MONEY_PARENT?.trim();
  const secret = process.env.SYLPHX_MONEY_API_KEY?.trim();
  if (!base || !parent || !environmentPath.test(parent) || !secret) {
    cached = undefined;
    return Promise.resolve("unconfigured");
  }

  if (cached?.config.base === base && cached.config.parent === parent &&
      cached.config.secret === secret && performance.now() < cached.expiresAt) {
    return cached.result;
  }

  const config = { base, parent, secret };
  // Infinity shares the pending promise. A superseded probe can only update its own entry.
  const entry = cached = {
    config,
    expiresAt: Infinity,
    result: probe(config).then((result) => {
      entry.expiresAt = performance.now() + (result === "ok" ? 60_000 : 15_000);
      return result;
    }),
  };
  return entry.result;
}

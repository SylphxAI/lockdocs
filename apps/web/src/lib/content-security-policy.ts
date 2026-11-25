import { httpsOrigin } from "./http";
import { signalConfig } from "./analytics-config";

const googleTag = "https://www.googletagmanager.com";
const googleCollection = [
  "https://www.google-analytics.com",
  "https://region1.google-analytics.com",
  "https://analytics.google.com",
  "https://region1.analytics.google.com",
  "https://www.google.com",
  "https://www.googleadservices.com",
  "https://googleads.g.doubleclick.net",
  "https://stats.g.doubleclick.net",
];

export function documentPolicy(nonce: string, development: boolean) {
  const authOrigins = [...new Set([process.env.SYLPHX_AUTH_URL, process.env.LOCKDOCS_TENANT_AUTH_ORIGIN]
    .map(httpsOrigin).filter(Boolean))];
  const connectSources = new Set([
    "'self'", ...authOrigins, "https://api.sylphx.com", signalConfig()?.signalApiOrigin, ...googleCollection,
    ...(development ? ["ws://localhost:*", "ws://127.0.0.1:*"] : []),
  ].filter(Boolean));
  // Google scripts execute only in the separate, consent-gated document below.
  return [
    "default-src 'none'",
    "base-uri 'none'",
    "object-src 'none'",
    "frame-ancestors 'none'",
    // Vite injects CSS and uses a same-host WebSocket for HMR in development only.
    "script-src 'self' 'nonce-" + nonce + "'",
    "style-src 'self'" + (development ? " 'unsafe-inline'" : " 'nonce-" + nonce + "'"),
    "font-src 'self' data:",
    "img-src 'self' data:",
    "manifest-src 'self'",
    "connect-src " + [...connectSources].join(" "),
    "frame-src 'self'",
    // Chromium can apply form-action to the final location as well as the POST URL.
    ["form-action", "'self'", "https://checkout.stripe.com", "https://billing.stripe.com", "http://127.0.0.1:*", ...authOrigins].join(" "),
  ].join("; ");
}

export const analyticsFramePolicy = [
  "default-src 'none'",
  "base-uri 'none'",
  "object-src 'none'",
  "frame-ancestors 'self'",
  "form-action 'none'",
  "script-src " + googleTag,
  "connect-src " + googleTag + " " + googleCollection.join(" "),
  "img-src " + googleTag + " " + googleCollection.join(" "),
].join("; ");

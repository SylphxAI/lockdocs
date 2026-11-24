import catalogue from "../../../../config/money-catalogue.json";
import {
  appOrigin, authConfigured, getSession, httpsOrigin, isVerifiedSession, moneyAccessToken,
  privateJson, privateRedirect, providerFetch, sameOrigin, type Session,
} from "./auth";
import { bindCheckoutAttribution, checkoutConsent } from "./checkout-attribution";
import { sendsGpc } from "./analytics-policy";
import { consent, type Choice } from "./analytics-policy";
import { measurementId } from "./analytics-config";
import { pricingCheckoutSurface } from "./analytics-events";
import { pricingPath, signInHref } from "./auth-entry";

const feature = "lockdocs_pro_seat_year";
const product = catalogue.spec.products.find((item) => item.key === feature);
const price = product?.prices.find((item) => item.key === feature);
const tokenProduct = product?.licence.token_product;

function config() {
  const base = httpsOrigin(process.env.SYLPHX_MONEY_URL);
  const origin = appOrigin();
  const parent = process.env.SYLPHX_MONEY_PARENT;
  const key = process.env.SYLPHX_PUBLISHABLE_KEY;
  if (!origin || !base || !key || !parent || !/^orgs\/[A-Za-z0-9_-]+\/projects\/[A-Za-z0-9_-]+\/envs\/[A-Za-z0-9_-]+$/.test(parent)) return null;
  return { base, origin, parent, key };
}

type MoneyConfig = NonNullable<ReturnType<typeof config>>;

function checkoutConfig() {
  const cfg = config();
  const secret = process.env.SYLPHX_MONEY_API_KEY;
  const termsText = process.env.LOCKDOCS_CCR_TERMS_TEXT?.trim();
  const termsVersion = process.env.LOCKDOCS_CCR_TERMS_VERSION?.trim();
  if (!cfg || !secret || !price || !termsText || Array.from(termsText).length > 1200 ||
    !termsVersion || Array.from(termsVersion).length > 64 || !authConfigured()) return null;
  return { ...cfg, price, secret, termsText, termsVersion };
}

/** Indicates configuration, not whether Money or the payment processor is reachable. */
export function billingConfigured() {
  return checkoutConfig() !== null;
}

function resource(cfg: MoneyConfig, operation: string, endUser = true) {
  return new URL((endUser ? "/v1/money/" : "/v1/") + cfg.parent + "/" + operation, cfg.base);
}

async function endUserHeaders(cfg: MoneyConfig, session: Session) {
  const token = await moneyAccessToken(session);
  return token ? { authorization: "Bearer " + token, "x-sylphx-key": cfg.key, accept: "application/json" } : null;
}

function stripeUrl(url: unknown, host: "checkout.stripe.com" | "billing.stripe.com") {
  if (typeof url !== "string") return null;
  try {
    const location = new URL(url);
    return location.origin === "https://" + host && !location.username && !location.password ? location : null;
  } catch {
    return null;
  }
}

export async function checkout(request: Request) {
  const cfg = checkoutConfig();
  if (!cfg) return privateJson({ error: "billing_unavailable" }, 503);
  if (!sameOrigin(request, cfg.origin)) return privateJson({ error: "invalid_origin" }, 403);
  const session = await getSession(request);
  if (!session) return privateRedirect(new URL(signInHref("/pricing"), cfg.origin));

  const contentType = request.headers.get("content-type")?.split(";", 1)[0].trim().toLowerCase();
  if (contentType !== "application/x-www-form-urlencoded" && contentType !== "multipart/form-data") {
    return privateJson({ error: "invalid_currency" }, 400);
  }
  let currency: FormDataEntryValue | null;
  const ga = measurementId();
  let choice: Choice = "reject";
  try {
    const form = await request.formData();
    currency = form.getAll("currency").length === 1 ? form.get("currency") : null;
    // Pricing is the only checkout form. Requests without the new field still start there.
    const surfaces = form.getAll("entry_surface");
    if (surfaces.length > 0 && (surfaces.length !== 1 || surfaces[0] !== pricingCheckoutSurface)) {
      return privateJson({ error: "invalid_entry_surface" }, 400);
    }
    if (ga && !sendsGpc(request)) choice = checkoutConsent(form);
  } catch {
    return privateJson({ error: "invalid_currency" }, 400);
  }
  if (typeof currency !== "string" || !Object.hasOwn(cfg.price.unit_amounts, currency)) {
    return privateJson({ error: "invalid_currency" }, 400);
  }
  if (!isVerifiedSession(session)) return privateJson({ error: "authentication_required" }, 401);
  const backToPricing = new URL(pricingPath(currency), cfg.origin);
  try {
    const response = await providerFetch(resource(cfg, "checkout_sessions", false), {
      method: "POST",
      headers: { authorization: "Bearer " + cfg.secret, "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({
        subject: { end_user: session.id },
        line_items: [{ price: cfg.price.key, quantity: 1 }],
        currency_code: currency,
        metadata: { entry_surface: pricingCheckoutSurface },
        success_url: new URL("/dashboard?billing=return", cfg.origin).href,
        cancel_url: backToPricing.href,
        consent_collection: { terms_required: true, terms_text: cfg.termsText, terms_version: cfg.termsVersion },
      }),
    });
    if (response.ok) {
      const body = await response.json();
      const location = stripeUrl(body?.url, "checkout.stripe.com");
      if (location) {
        if (ga && consent(choice).analytics_storage === "granted") {
          const prefix = cfg.parent + "/checkout_sessions/";
          const id = typeof body.name === "string" && body.name.startsWith(prefix) ? body.name.slice(prefix.length) : "";
          // Money normally returns a resource name; also accept an explicit ID if no name is supplied.
          const bindingId = body.name == null && typeof body.checkout_session_id === "string" ? body.checkout_session_id : id;
          void bindCheckoutAttribution(request, cfg, bindingId, choice, ga);
        }
        return privateRedirect(location);
      }
    }
  } catch { /* Fail closed on upstream errors and unexpected data. */ }
  backToPricing.searchParams.set("billing", "unavailable");
  return privateRedirect(backToPricing);
}

export async function portal(request: Request) {
  const cfg = config();
  if (!cfg || !authConfigured()) return privateJson({ error: "billing_unavailable" }, 503);
  if (!sameOrigin(request, cfg.origin)) return privateJson({ error: "invalid_origin" }, 403);
  const session = await getSession(request);
  if (!session) return privateRedirect(new URL(signInHref("/dashboard"), cfg.origin));
  try {
    const headers = await endUserHeaders(cfg, session);
    if (headers) {
      const response = await providerFetch(resource(cfg, "portal_sessions:userCreate"), {
        method: "POST",
        headers: { ...headers, "content-type": "application/json" },
        body: JSON.stringify({ subject: { end_user: session.id }, return_url: new URL("/dashboard", cfg.origin).href }),
      });
      if (response.ok) {
        const location = stripeUrl((await response.json())?.url, "billing.stripe.com");
        if (location) return privateRedirect(location);
      } else if (response.status === 412) {
        const problem = await response.json();
        if (problem?.code === "INVALID_STATE" && Array.isArray(problem.details) &&
          problem.details.length === 1 && problem.details[0]?.reason === "no_customer") {
          return privateRedirect(new URL("/dashboard?billing=no_customer", cfg.origin));
        }
      }
    }
  } catch { /* Fail closed on upstream errors and unexpected data. */ }
  return privateRedirect(new URL("/dashboard?billing=unavailable", cfg.origin));
}

export type Entitlement =
  | { status: "entitled"; entitled: true }
  | { status: "not_entitled" | "unavailable" | "unauthenticated"; entitled: false };

/** No expiry is asserted: Money returns an authoritative boolean at read time, not an expiry. */
export async function getEntitlement(session: Session | null): Promise<Entitlement> {
  if (!session) return { status: "unauthenticated", entitled: false };
  const unavailable = { status: "unavailable", entitled: false } as const;
  const cfg = config();
  if (!cfg) return unavailable;
  try {
    const headers = await endUserHeaders(cfg, session);
    if (!headers) return unavailable;
    const url = resource(cfg, "entitlement_grants:getMine");
    url.searchParams.set("feature", feature);
    const response = await providerFetch(url, { headers });
    if (!response.ok) return unavailable;
    const body = await response.json();
    if (body?.subject_id !== session.id || typeof body.entitled !== "boolean") return unavailable;
    return body.entitled ? { status: "entitled", entitled: true } : { status: "not_entitled", entitled: false };
  } catch {
    return unavailable;
  }
}

export type LicenceToken = { token: string; token_product: string; issue_time: string; expire_time: string; seats: number };
export type ListMineResult =
  | { status: "ok"; licence_tokens: LicenceToken[]; next_page_token: string }
  | { status: "unauthenticated" | "unavailable" };

function licence(value: unknown, expectedProduct: string): LicenceToken | null {
  if (!value || typeof value !== "object") return null;
  const item = value as Partial<LicenceToken>;
  const { token, token_product, issue_time, expire_time, seats } = item;
  if (typeof token !== "string" || !token || token.length > 16_384 || token_product !== expectedProduct ||
    typeof issue_time !== "string" || !Number.isFinite(Date.parse(issue_time)) ||
    typeof expire_time !== "string" || !(Date.parse(expire_time) > Date.now()) ||
    typeof seats !== "number" || !Number.isSafeInteger(seats) || seats < 1) return null;
  return { token, token_product, issue_time, expire_time, seats };
}

/** Account-scoped offline credentials. Keep the result private; only the CLI exchange may deliver one. */
export async function listMine(session: Session | null, options: { pageToken?: string } = {}): Promise<ListMineResult> {
  if (!session) return { status: "unauthenticated" };
  const unavailable = { status: "unavailable" } as const;
  const cfg = config();
  if (!cfg || !tokenProduct || (options.pageToken?.length ?? 0) > 2048) return unavailable;
  try {
    const headers = await endUserHeaders(cfg, session);
    if (!headers) return unavailable;
    const url = resource(cfg, "licence_tokens:listMine");
    url.searchParams.set("filter", 'end_user = "' + session.id + '" AND token_product = "' + tokenProduct + '"');
    if (options.pageToken) url.searchParams.set("page_token", options.pageToken);
    const response = await providerFetch(url, { headers });
    if (!response.ok) return unavailable;
    const body = await response.json();
    if (!Array.isArray(body?.licence_tokens) || typeof body.next_page_token !== "string") return unavailable;
    const licence_tokens: LicenceToken[] = [];
    for (const item of body.licence_tokens) {
      const parsed = licence(item, tokenProduct);
      if (!parsed) return unavailable;
      licence_tokens.push(parsed);
    }
    return { status: "ok", licence_tokens, next_page_token: body.next_page_token };
  } catch {
    return unavailable;
  }
}

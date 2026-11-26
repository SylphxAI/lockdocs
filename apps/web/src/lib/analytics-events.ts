import catalogue from "../../../../config/money-catalogue.json";
import { formats } from "./lockfile-formats";

const proSurfaces = ["playground_upgrade_cta", "playground_report_preview", "home_pro_band"] as const;
export type ProSurface = (typeof proSurfaces)[number];
const articleSurfaces = {
  "/compare": "article_compare",
  "/benchmarks": "article_benchmarks",
  "/guide": "article_guide",
  "/pro": "article_pro",
  "/guide/setup": "article_guide_setup",
  "/guide/ecosystems": "article_guide_ecosystems",
  "/guide/fetch": "article_guide_fetch",
  "/guide/how-it-works": "article_guide_how_it_works",
  "/reference": "article_reference",
  "/reference/tools": "article_reference_tools",
  "/capabilities": "article_capabilities",
  "/docs": "article_docs",
  "/zh-TW/docs": "article_docs_zh_tw",
} as const;
export type ArticleSurface = (typeof articleSurfaces)[keyof typeof articleSurfaces];
export function articleSurface(path: string): ArticleSurface | undefined {
  return Object.hasOwn(articleSurfaces, path) ? articleSurfaces[path as keyof typeof articleSurfaces] : undefined;
}

export const proPrice = catalogue.spec.products[0].prices[0];
export const billingCurrencies = Object.keys(proPrice.unit_amounts) as (keyof typeof proPrice.unit_amounts)[];
export const pricingCheckoutSurface = "pricing";
export const checkoutEntrySurfaces = [
  pricingCheckoutSurface, ...proSurfaces, "context7_alternative", "guide_cursor_claude_code", ...Object.values(articleSurfaces),
] as const;
export type CheckoutSurface = (typeof checkoutEntrySurfaces)[number];

export function knownCheckoutSurface(value: unknown): CheckoutSurface | undefined {
  return checkoutEntrySurfaces.find(surface => surface === value);
}

const entrySurface = { entry_surface: proSurfaces };
const eventFields = {
  lockfile_resolved: {
    lockfile_format: formats,
    source_kind: ["sample", "pasted", "file"],
    package_count: (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value) && value >= 0,
  },
  command_copied: {
    surface: ["home_install", "playground_mcp", "docs_code", "playground_cli", "playground_upgrade"],
    command_kind: ["npx", "uvx", "pip", "mcp_config", "code", "cli"],
  },
  paywall_view: entrySurface,
  pro_cta_clicked: { entry_surface: [...checkoutEntrySurfaces.filter(surface => surface !== pricingCheckoutSurface),
    "pricing_proof_upgrade_report", "pricing_proof_private_sources"] },
  pricing_buy_clicked: { entry_surface: checkoutEntrySurfaces, currency: billingCurrencies, locale: ["en", "zh-TW"] },
  upgrade_preview_switched: { example_id: ["next", "pydantic", "zod", "react"] },
  cli_authorized: { entry_surface: ["cli_authorize"] },
} as const;

type EventName = keyof typeof eventFields;
type FieldValue<T> = T extends readonly (infer Value)[] ? Value : number;
export type EventParams<Name extends EventName> = {
  [Key in keyof (typeof eventFields)[Name]]: FieldValue<(typeof eventFields)[Name][Key]>;
};
export type TrackEvent = <Name extends EventName>(
  event: Name, params: EventParams<NoInfer<Name>>, options?: { requireExplicitConsent: boolean },
) => boolean;

// Rebuild from the allowlist at the transport boundary. Never forward a caller's object as-is.
export function behavioralParams(event: string, params: Readonly<Record<string, unknown>>) {
  if (!Object.hasOwn(eventFields, event)) return null;
  const safe: Record<string, unknown> = {};
  for (const [field, allowed] of Object.entries(eventFields[event as EventName])) {
    const value = params[field];
    if (!(typeof allowed === "function" ? allowed(value) : (allowed as readonly unknown[]).includes(value))) return null;
    safe[field] = value;
  }
  return safe;
}

export function beginCheckoutParams(currency: unknown, entrySurface: unknown = pricingCheckoutSurface) {
  const amounts = proPrice.unit_amounts;
  if (typeof currency !== "string" || !Object.hasOwn(amounts, currency)) return null;
  const minor = Number(amounts[currency as keyof typeof amounts]);
  if (!Number.isSafeInteger(minor) || minor <= 0) return null;
  const value = minor / 100;
  return {
    value, currency, plan_sku: proPrice.key, entry_surface: knownCheckoutSurface(entrySurface) ?? pricingCheckoutSurface,
    items: [{ item_id: proPrice.key, item_name: "Lockdocs Pro", quantity: 1, price: value }],
  };
}

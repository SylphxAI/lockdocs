import catalogue from "../../../../config/money-catalogue.json";

export const proPrice = catalogue.spec.products[0].prices[0];

export function beginCheckoutParams(currency: unknown) {
  const amounts = proPrice.unit_amounts;
  if (typeof currency !== "string" || !Object.hasOwn(amounts, currency)) return null;
  const minor = Number(amounts[currency as keyof typeof amounts]);
  if (!Number.isSafeInteger(minor) || minor <= 0) return null;
  const value = minor / 100;
  return {
    value, currency, plan_sku: proPrice.key, entry_surface: "pricing",
    items: [{ item_id: proPrice.key, item_name: "Lockdocs Pro", quantity: 1, price: value }],
  };
}

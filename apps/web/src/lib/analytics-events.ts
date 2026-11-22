export type FunnelEvent = "pricing_view" | "begin_checkout";
export const items = [{ item_id: "lockdocs_pro_seat_year", item_name: "Lockdocs Pro", quantity: 1 }];
export const pathByEvent: Record<FunnelEvent, string> = {
  pricing_view: "/pricing", begin_checkout: "/pricing",
};

import { getSession } from "./auth";
import { billingConfigured, getEntitlement } from "./money";

export async function accountDetails(request: Request) {
  const session = await getSession(request);
  const entitlement = await getEntitlement(session);
  return { user: session ? { name: session.name || session.email || "Signed in" } : null, entitlement, billing: billingConfigured() };
}

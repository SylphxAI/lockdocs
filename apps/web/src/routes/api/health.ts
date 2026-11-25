import { moneyHealth } from "../../lib/money-health";

export async function GET() {
  return Response.json({ status: "ok", money: await moneyHealth() }, { headers: { "Cache-Control": "no-store" } });
}

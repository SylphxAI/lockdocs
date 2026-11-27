import { A } from "@solidjs/router";
import { annualBilling, perYear } from "@/lib/pro-plan";
import type { PurchaseResume } from "@/lib/purchase-resume";
import { useSiteLanguage } from "@/lib/site-language";

// Read-only reminder of the plan in progress. Nothing here starts checkout:
// the buyer returns to pricing and continues to secure checkout there.
export function PurchaseSummary(props: { purchase: PurchaseResume }) {
  const { text } = useSiteLanguage();
  return <section class="purchase-resume" aria-labelledby="purchase-resume-title">
    <div class="plan-label">{text("YOUR PLAN", "你選擇的方案")}</div>
    <h2 id="purchase-resume-title">Lockdocs Pro · {props.purchase.price}{text(...perYear)}</h2>
    <p class="quiet">{text(...annualBilling)}</p>
    <p class="quiet">{text("After you sign in, you return to pricing. Nothing is charged until you continue to secure checkout there.", "登入後會回到價格頁面。在該頁繼續前往安全結帳前，不會收取任何費用。")}</p>
    <A href={props.purchase.query ? props.purchase.path + "?" + props.purchase.query : props.purchase.path}>{text("Back to pricing", "返回價格頁面")}</A>
  </section>;
}

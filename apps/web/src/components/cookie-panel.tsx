import { Show } from "solid-js";
import type { StoredChoice } from "@/lib/signal-privacy";
import { useSiteLanguage } from "@/lib/site-language";

type CookiePanelProps = {
  protectedByGpc: boolean;
  choose: (choice: StoredChoice) => void;
  onEscape: () => void;
  ref: (element: HTMLElement) => void;
};

export function CookiePanel(props: CookiePanelProps) {
  const { text } = useSiteLanguage();
  return <section id="cookie-settings" class="cookie-panel" aria-labelledby="cookie-heading" tabIndex={-1}
    ref={props.ref} onKeyDown={event => { if (event.key === "Escape") props.onEscape(); }}>
    <div class="cookie-copy">
      <h2 id="cookie-heading">{text("Cookie settings", "Cookie 設定")}</h2>
      <p><Show when={props.protectedByGpc} fallback={text("Optional cookies: analytics and personalized ads.", "選用 Cookie 用於分析與個人化廣告。")}>
        <span role="status">{text("Global Privacy Control blocks optional cookies.", "全域隱私控制（GPC）已封鎖選用 Cookie。")}</span>
      </Show>{" "}<a href="/legal/privacy">{text("Privacy policy", "隱私權政策")}</a></p>
    </div>
    <div class="cookie-actions">
      <button type="button" class="cookie-choice" onClick={() => props.choose("denied")}>{text("Reject optional", "拒絕選用項目")}</button>
      <button type="button" class="cookie-choice" disabled={props.protectedByGpc} onClick={() => props.choose("analytics_only")}>{text("Analytics only", "僅限分析")}</button>
      <button type="button" class="cookie-choice" disabled={props.protectedByGpc} onClick={() => props.choose("granted")}>{text("Allow analytics & ads", "允許分析與廣告")}</button>
    </div>
  </section>;
}

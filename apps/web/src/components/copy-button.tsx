import { createSignal } from "solid-js";
import { Check, Copy } from "lucide-solid";
import { useCookieSettings } from "@/components/cookie-settings";
import type { EventParams } from "@/lib/analytics-events";
import { useSiteLanguage } from "@/lib/site-language";

export function CopyButton(props: { value: string; label?: string; analytics?: EventParams<"command_copied"> }) {
  const { trackEvent } = useCookieSettings();
  const { text } = useSiteLanguage();
  const [status, setStatus] = createSignal<"copied" | "failed">();
  const label = () => props.label ?? text("Copy", "複製");

  async function copy() {
    const analytics = props.analytics ?? { surface: "docs_code", command_kind: "code" };
    try { await navigator.clipboard.writeText(props.value); }
    catch { setStatus("failed"); return; }
    setStatus("copied");
    trackEvent("command_copied", analytics);
  }
  return <><button type="button" class="copy-button" aria-label={label()} title={label()} onClick={copy}>
    {status() === "copied" ? <Check size={16}/> : <Copy size={16}/>}
  </button><span class="sr-only" role="status">{status() === "copied" ? text("Copied", "已複製") : status() === "failed" ? text("Select the text to copy it.", "請選取文字後手動複製。") : ""}</span></>;
}

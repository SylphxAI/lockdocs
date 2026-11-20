
import { createSignal } from 'solid-js';
import { Check, Copy } from 'lucide-solid';
export function CopyButton({ value, label = 'Copy' }: { value:string; label?:string }) {
  const [status, setStatus] = createSignal('');
  return <><button type="button" class="copy-button" aria-label={label} title={label} onClick={async () => {
    try { await navigator.clipboard.writeText(value); setStatus('Copied'); }
    catch { setStatus('Select the text to copy it.'); }
  }}>{status() === 'Copied' ? <Check size={16}/> : <Copy size={16}/>}</button><span class="sr-only" role="status">{status()}</span></>;
}

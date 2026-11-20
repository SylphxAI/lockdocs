
import { createSignal } from 'solid-js';
import { Check, Copy } from 'lucide-solid';
export function CopyButton(props: { value:string; label?:string }) {
  const [status, setStatus] = createSignal('');
  return <><button type="button" class="copy-button" aria-label={props.label ?? 'Copy'} title={props.label ?? 'Copy'} onClick={async () => {
    try { await navigator.clipboard.writeText(props.value); setStatus('Copied'); }
    catch { setStatus('Select the text to copy it.'); }
  }}>{status() === 'Copied' ? <Check size={16}/> : <Copy size={16}/>}</button><span class="sr-only" role="status">{status()}</span></>;
}

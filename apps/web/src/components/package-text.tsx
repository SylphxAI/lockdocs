import { For } from "solid-js";

// A real element boundary prevents Cloudflare from mistaking name@version for email.
// Keep the literal @ so selection and copy still produce the original text.
export function PackageText(props: { text: string }) {
  return <For each={props.text.split(/(?<=[\w.-])@(?=[vV]?\d)/)}>{(part, index) =>
    <>{index() > 0 && <span>@</span>}{part}</>
  }</For>;
}

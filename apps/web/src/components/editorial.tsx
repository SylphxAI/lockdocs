import { For, type JSX, type ParentProps } from "solid-js";
import { CopyButton } from "@/components/copy-button";
import { PackageText } from "@/components/package-text";
import { JsonLd } from "@/components/json-ld";
import { faqPage, type Faq } from "@/lib/faq-schema";
import "./editorial.css";

export function EditorialHero(props: ParentProps<{ label: string; title: string; intro: string; actions?: JSX.Element }>) {
  return <header class="wrap editorial-hero">
    <div class="editorial-hero-copy">
      <p class="eyebrow"><span class="signal"/> {props.label}</p>
      <h1>{props.title}</h1>
      <p class="editorial-lead">{props.intro}</p>
      <div class="button-row editorial-hero-actions">{props.actions}</div>
    </div>
    {props.children}
  </header>;
}

export function CodeExample(props: { label: string; code: string }) {
  return <figure class="editorial-code">
    <figcaption><span>{props.label}</span><CopyButton value={props.code} label={"Copy " + props.label}/></figcaption>
    <pre aria-label={props.label} tabindex="0"><code><PackageText text={props.code}/></code></pre>
  </figure>;
}

export function EditorialFaq(props: { items: readonly Faq[]; url: string; heading?: string }) {
  return <section id="faq" class="editorial-section" aria-labelledby="faq-title">
    <h2 id="faq-title">{props.heading ?? "Frequently asked questions"}</h2>
    <div class="editorial-faqs">
      <For each={props.items}>{item => <details>
        <summary>{item.question}</summary>
        <p>{item.answer}</p>
        <For each={item.links}>{link => <a class="doc-source editorial-faq-link" href={link.href}>{link.label}</a>}</For>
      </details>}</For>
    </div>
    <JsonLd data={faqPage(props.url, props.items)}/>
  </section>;
}

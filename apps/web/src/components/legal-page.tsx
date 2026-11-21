import type { ParentProps } from "solid-js";
import { PageMeta } from "./page-meta";

export function LegalPage(props: ParentProps<{ title: string; description: string }>) {
  return <div class="wrap content-section">
    <PageMeta title={`${props.title} · Lockdocs`} description={props.description}/>
    <article class="doc-body legal-body">
      <div class="kicker"><span/> LOCKDOCS LEGAL</div>
      <h1>{props.title}</h1>
      <p>{props.description}</p>
      {props.children}
    </article>
  </div>;
}

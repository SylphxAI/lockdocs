import { createMemo, For } from "solid-js";
import { pageLinks } from "@/lib/locales";
import { Link, Meta, Title } from "@solidjs/meta";
import { useLocation } from "@solidjs/router";
import { canonicalOrigin } from "@/lib/indexing";

export function PageMeta(props: {title: string; description: string; unlisted?: boolean | "noindex"}) {
  const location = useLocation();
  const links = createMemo(() => pageLinks(location.pathname));
  return <>
    <Title>{props.title}</Title>
    <Meta name="description" content={props.description}/>
    {props.unlisted
      ? <Meta name="robots" content={props.unlisted === "noindex" ? "noindex" : "noindex, nofollow"}/>
      : <>
          <Link rel="canonical" href={links().canonical}/>
          <For each={links().alternates}>{alternate =>
            <Link rel="alternate" hreflang={alternate.locale} href={alternate.href}/>
          }</For>
          <Meta property="og:type" content="website"/>
          <Meta property="og:url" content={links().canonical}/>
          <Meta property="og:title" content={props.title}/>
          <Meta property="og:description" content={props.description}/>
          <Meta property="og:image" content={canonicalOrigin + "/og-image.png"}/>
          <Meta name="twitter:card" content="summary_large_image"/>
          <Meta name="twitter:image" content={canonicalOrigin + "/og-image.png"}/>
        </>}
  </>;
}

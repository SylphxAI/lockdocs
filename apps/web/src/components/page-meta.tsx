import { Link, Meta, Title } from "@solidjs/meta";
import { useLocation } from "@solidjs/router";
import { canonicalOrigin } from "@/lib/indexing";

export function PageMeta(props: {title: string; description: string; unlisted?: boolean}) {
  const location = useLocation();
  return <>
    <Title>{props.title}</Title>
    <Meta name="description" content={props.description}/>
    {props.unlisted
      ? <Meta name="robots" content="noindex, nofollow"/>
      : <Link rel="canonical" href={canonicalOrigin + location.pathname}/>}
  </>;
}

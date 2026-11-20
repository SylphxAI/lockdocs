import { Title, Meta } from "@solidjs/meta";
export function PageMeta(props: {title: string; description: string; unlisted?: boolean}) {
  return <><Title>{props.title}</Title><Meta name="description" content={props.description}/>{props.unlisted && <Meta name="robots" content="noindex, nofollow"/>}</>;
}

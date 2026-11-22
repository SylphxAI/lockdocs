export function JsonLd(props: { data: Record<string, unknown> }) {
  // A literal '<' can end a script element. Escaping '@' also keeps package pins
  // out of Cloudflare's email rewriting without changing the parsed JSON.
  const json = () => JSON.stringify(props.data).replaceAll("<", "\u003c").replaceAll("@", "\u0040");
  return <script type="application/ld+json" textContent={json()}/>;
}

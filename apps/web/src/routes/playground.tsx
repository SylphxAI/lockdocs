import { JsonLd } from "@/components/json-ld";
import { PageMeta } from "@/components/page-meta";
import { Studio } from "@/components/studio";
import { canonicalOrigin } from "@/lib/indexing";
import { useSiteLanguage } from "@/lib/site-language";

export default function Playground() {
  const { href, locale, text } = useSiteLanguage();
  const name = () => text("Lockfile Playground", "lockfile 試用工具");
  const description = () => text("Resolve exact dependency versions from a lockfile in your browser. Pasted text stays in this browser.", "直接在瀏覽器中從 lockfile 查出確切的套件版本。你貼上的內容只會留在這個瀏覽器裡。");
  const url = () => canonicalOrigin + href("/playground");
  return <div class="wrap content-section">
    <PageMeta title={name() + " · Lockdocs"} description={description()}/>
    <JsonLd data={{
      "@context": "https://schema.org",
      "@type": "WebApplication",
      name: name(),
      description: description(),
      url: url(),
      inLanguage: locale(),
      applicationCategory: "DeveloperApplication",
      browserRequirements: "Requires JavaScript",
      isAccessibleForFree: true,
      offers: { "@type": "Offer", url: url(), price: 0, priceCurrency: "USD" },
    }}/>
    <div class="page-intro">
      <div class="kicker"><span/> {text("WORKBENCH / 01", "試用工具 / 01")}</div>
      <h1>{name()}</h1>
      <p>{text("Resolve the exact versions in a lockfile and see what changes when you upgrade. Pasted text stays in this browser.", "從 lockfile 查出確切的套件版本，再看看升級會帶來哪些變更。你貼上的內容只會留在這個瀏覽器裡。")}</p>
    </div>
    <Studio/>
  </div>;
}

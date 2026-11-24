import { PageMeta } from "@/components/page-meta";
import { Studio } from "@/components/studio";

import { useSiteLanguage } from "@/lib/site-language";

export default function Playground() {
  const { text } = useSiteLanguage();
  return <div class="wrap content-section"><PageMeta title={text("Lockfile Playground · Lockdocs", "lockfile 試用工具 · Lockdocs")} description={text("Inspect exact versions from a lockfile, generate MCP setup and explore real release citations.", "從 lockfile 確認套件版本、取得 MCP 設定，並查看實際發行版本的原始碼出處。")}/><div class="page-intro"><div class="kicker"><span/> {text("WORKBENCH / 01", "試用工具 / 01")}</div><h1>{text("Lockfile Playground", "lockfile 試用工具")}</h1><p>Resolve the exact versions in a lockfile and inspect what changes when you upgrade. Pasted text stays in this browser.</p></div><Studio/></div>;
}

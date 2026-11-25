import { PageMeta } from "@/components/page-meta";
import { Studio } from "@/components/studio";

import { useSiteLanguage } from "@/lib/site-language";

export default function Playground() {
  const { text } = useSiteLanguage();
  return <div class="wrap content-section"><PageMeta title={text("Lockfile Playground · Lockdocs", "lockfile 試用工具 · Lockdocs")} description={text("Inspect exact versions from a lockfile, generate MCP setup and explore real release citations.", "從 lockfile 確認套件版本、取得 MCP 設定，並查看實際發行版本的原始碼出處。")}/><div class="page-intro"><div class="kicker"><span/> {text("WORKBENCH / 01", "試用工具 / 01")}</div><h1>{text("Lockfile Playground", "lockfile 試用工具")}</h1><p>{text("Resolve the exact versions in a lockfile and see what changes when you upgrade. Pasted text stays in this browser.", "從 lockfile 查出確切的套件版本，再看看升級會帶來哪些變更。你貼上的內容只會留在這個瀏覽器裡。")}</p></div><Studio/></div>;
}

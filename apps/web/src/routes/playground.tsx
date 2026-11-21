import { PageMeta } from "@/components/page-meta";
import { Studio } from "@/components/studio";

export default function Playground() {
  return <div class="wrap content-section"><PageMeta title="Lockfile Playground · Lockdocs" description="Inspect exact versions from a lockfile, generate MCP setup and explore real release citations."/><div class="page-intro"><div class="kicker"><span/> WORKBENCH / 01</div><h1>Lockfile Playground</h1><p>Resolve the exact versions in a lockfile and inspect what changes when you upgrade. Pasted text stays in this browser.</p></div><Studio/></div>;
}

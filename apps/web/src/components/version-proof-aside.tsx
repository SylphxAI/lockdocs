import { A } from "@solidjs/router";
import { ArrowRight } from "lucide-solid";
import { createSignal, For } from "solid-js";
import { PackageText } from "@/components/package-text";
import { useSiteLanguage } from "@/lib/site-language";
import { upgradeIds, upgradePreviewPath, upgrades, type UpgradeId } from "@/lib/upgrade-examples";
import "./version-proof-aside.css";

function VersionCall(props: { label: string; call: string; source: string }) {
  return <div class="version-proof-call">
    <dt>{props.label}</dt>
    <dd><code>{props.call}</code><span class="version-proof-citation"><PackageText text={props.source}/></span></dd>
  </div>;
}

export function VersionProofAside() {
  const { carryCampaign, href } = useSiteLanguage();
  const [selected, select] = createSignal<UpgradeId>("next");
  return <aside class="editorial-aside version-proof" aria-labelledby="version-proof-title">
    <p class="eyebrow" id="version-proof-title">VERSION-SENSITIVE PROOF</p>
    <div class="version-proof-switcher" role="group" aria-label="Upgrade ecosystem">
      <For each={upgradeIds}>{id =>
        <button type="button" aria-pressed={selected() === id} onClick={() => select(id)}>{upgrades[id].name}</button>
      }</For>
    </div>
    <div class="version-proof-examples">
      <For each={upgradeIds}>{id => {
        const example = upgrades[id];
        return <dl class="version-proof-example" aria-hidden={selected() !== id}>
          <VersionCall label="Before" call={example.before} source={example.oldSource}/>
          <VersionCall label="After" call={example.after} source={example.newSource}/>
        </dl>;
      }}</For>
    </div>
    <A class="version-proof-link" href={carryCampaign(upgradePreviewPath(href("/playground"), selected()))}>
      Inspect {upgrades[selected()].name} in playground <ArrowRight size={15} aria-hidden="true"/>
    </A>
  </aside>;
}

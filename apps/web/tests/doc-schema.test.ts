import assert from "node:assert/strict";
import { test } from "node:test";
import { docMetadata } from "../src/lib/doc-schema";
import { docPages } from "../src/lib/docs";

const origin = "https://lockdocs.sylphx.com";
const doc = (route: string) => {
  const page = docPages.find(page => page.route === route);
  assert.ok(page, route);
  return page;
};

test("guides, reference, capabilities and vision describe Lockdocs as technical articles", () => {
  const organization = { "@type": "Organization", name: "Sylphx Limited" };
  for (const [route, headline] of [
    ["/guide", "Quickstart"], ["/guide/setup", "Agent setup"], ["/guide/how-it-works", "How it works"],
    ["/guide/ecosystems", "Supported ecosystems"], ["/guide/fetch", "Fetching docs"],
    ["/reference", "CLI reference"], ["/reference/tools", "MCP tools"],
    ["/capabilities", "Capabilities"], ["/vision", "Vision"],
  ]) {
    const page = doc(route);
    assert.deepEqual(docMetadata(page, "en"), {
      title: headline + " · Lockdocs", description: page.description,
      schema: {
        "@context": "https://schema.org", "@type": "TechArticle", url: origin + route,
        headline, description: page.description, inLanguage: "en", author: organization, publisher: organization,
        about: { "@type": "SoftwareApplication", "@id": origin + "/#lockdocs", name: "Lockdocs",
          applicationCategory: "DeveloperApplication", operatingSystem: "Linux, macOS, Windows" },
      },
    }, route);
  }
});

test("docs hubs describe their own canonical collection in English and Traditional Chinese", () => {
  for (const [locale, route, name, description] of [
    ["en", "/docs", "Documentation", doc("/docs").description],
    ["zh-TW", "/zh-TW/docs", "文件總覽", "學會安裝 Lockdocs、設定 MCP，並查閱專案實際安裝版本的 API 文件。"],
  ] as const) {
    assert.deepEqual(docMetadata(doc("/docs"), locale), {
      title: name + " · Lockdocs", description,
      schema: { "@context": "https://schema.org", "@type": "CollectionPage", url: origin + route, name, description, inLanguage: locale },
    });
  }
});

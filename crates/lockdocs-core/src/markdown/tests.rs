use super::imports::example_imports;
use super::{clean_mdx, split, topic_heading};

#[test]
fn imports_and_ancestry_follow_logical_sections_before_chunking() {
    let md = format!("# Guide\n\nCommon introduction.\n\n## Setup\n\n```ts\nimport {{ build }} from '@sample/forms/next';\n```\n\n{}\n\n### Recipe\n\nInherited recipe.\n\n## Setup\n\nUnrelated setup.\n\n### Recipe\n\nIndependent recipe.\n", ("Long paragraph. ".repeat(100) + "\n\n").repeat(3));
    let sections = split(&md, "README");
    let chunks: Vec<_> = sections
        .iter()
        .filter(|s| s.heading.ends_with("Setup") && !s.text.contains("Unrelated"))
        .collect();
    assert!(chunks.len() > 1);
    assert!(chunks.iter().all(|s| s.imports == ["@sample/forms/next"]));
    let inherited = sections.iter().find(|s| s.text == "Inherited recipe.").unwrap();
    assert_eq!(inherited.imports, ["@sample/forms/next"]);
    assert!(inherited.parents.contains(&chunks[0].line));
    let independent = sections.iter().find(|s| s.text == "Independent recipe.").unwrap();
    assert!(independent.imports.is_empty());
    assert!(!independent.parents.contains(&chunks[0].line));
}

#[test]
fn imports_come_from_examples_and_override_only_their_package() {
    let md = "import X from 'component/plumbing';\n\n# Setup\n\n```js\nconst f = require('@sample/forms/next');\nconst other = import('other/setup');\n```\n\n## Override\n\n```ts\nimport { build } from '@sample/forms';\nexport { build } from '@sample/forms/unknown';\n// import x from 'ignored/comment';\nconst label = \"import x from 'ignored/string'\";\n```\n\n### Child\n\nAn inherited example.\n";
    let sections = split(md, "README");
    assert!(sections[0].imports.is_empty());
    let child = sections.last().unwrap();
    assert_eq!(child.imports, ["@sample/forms", "@sample/forms/unknown", "other/setup"]);
    assert_eq!(example_imports("```ts\nimport f from 'unfinished/example';"), ["unfinished/example"]);
}

#[test]
fn sections_follow_headings_and_skip_fences() {
    let md = "# Zod\n\nIntro.\n\n## Objects\n\n```ts\n# not a heading\nz.object({})\n```\n\n### strict\n\nText.\n\nSetext\n------\n\nMore.\n";
    let s = split(md, "README");
    let heads: Vec<&str> = s.iter().map(|x| x.heading.as_str()).collect();
    assert_eq!(
        heads,
        vec![
            "README › Zod",
            "README › Zod › Objects",
            "README › Zod › Objects › strict",
            "README › Zod › Setext"
        ]
    );
    assert!(s[1].text.contains("# not a heading"));
    assert_eq!(s[1].line, 5);
}

#[test]
fn mdx_exports_and_html_headings() {
    let mdx = "import { X } from \"x\";\n\nexport const title = \"Functions and directives\";\nexport const description = \"Custom directives.\";\n\n## Directives\n\nIntro.\n\n<h3 id=\"import-directive\">\n  <a href=\"#import-directive\">@import</a>\n</h3>\n\nUse `@import`.\n\n<h3 id=\"theme\"><a href=\"#theme\">@theme</a></h3>\n\nTokens.\n";
    let (text, title) = clean_mdx(mdx, true);
    assert_eq!(title.as_deref(), Some("Functions and directives"));
    assert_eq!(text.lines().count(), mdx.lines().count());
    let s = split(&text, title.as_deref().unwrap());
    let heads: Vec<&str> = s.iter().map(|x| x.heading.as_str()).collect();
    assert_eq!(
        heads,
        vec![
            "Functions and directives",
            "Functions and directives › Directives",
            "Functions and directives › Directives › @import",
            "Functions and directives › Directives › @theme"
        ]
    );
    assert!(s[0].text.contains("Custom directives."));
    assert_eq!(s[2].line, 10);
}

#[test]
fn heading_ids_and_generic_headings() {
    let s = split("# forwardRef\n\n## Usage {/*usage*/}\n\nText.\n\n### Props {#props}\n\nMore.\n", "doc");
    assert_eq!(s[0].heading, "doc › forwardRef › Usage");
    assert_eq!(topic_heading(&s[1].heading), "Usage");
    assert_eq!(topic_heading("generateMetadata › Parameters"), "generateMetadata");
}

#[test]
fn rst_headings() {
    let rst = "Title\n=====\n\nBody\n\nSub\n---\n\nx\n";
    let s = split(rst, "doc");
    assert_eq!(s.len(), 2);
    assert_eq!(s[1].heading, "doc › Title › Sub");
}

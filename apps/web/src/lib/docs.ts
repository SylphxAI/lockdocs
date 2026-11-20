

const entries = [
  ["/docs", "index.md", "Documentation", "Learn how Lockdocs reads your lockfile and serves documentation for the exact library versions in your project."],
  ["/guide", "guide/quickstart.md", "Quickstart", "Install Lockdocs and look up your first version-pinned API, from your terminal or coding agent."],
  ["/guide/setup", "guide/setup.md", "Agent setup", "Connect the local Lockdocs MCP server to your agent and verify that it is using the project’s lockfile."],
  ["/guide/how-it-works", "guide/how-it-works.md", "How it works", "How Lockdocs resolves pinned dependencies, indexes local documentation and cites exact source lines."],
  ["/guide/ecosystems", "guide/ecosystems.md", "Supported ecosystems", "Lockfile and language support for JavaScript, Python, Rust and Go."],
  ["/guide/fetch", "guide/fetch.md", "Fetching docs", "Fetch upstream material for pinned packages and keep it available offline."],
  ["/reference", "reference/cli.md", "CLI reference", "Lockdocs commands, options and operational details for the terminal."],
  ["/reference/tools", "reference/tools.md", "MCP tools", "The MCP contract for resolve, docs and api, with arguments and expected responses."],
  ["/benchmarks", "benchmarks.md", "Benchmarks", "Evaluate Lockdocs and Context7 on 105 version-sensitive questions, including correctness, latency and methodology."],
  ["/compare", "compare.md", "Compare", "Compare Lockdocs and Context7 for local, version-sensitive developer documentation."],
  ["/capabilities", "capabilities.md", "Capabilities", "See which Lockdocs capabilities work today and where its coverage stops."],
  ["/vision", "vision.md", "Vision", "The design direction for local, exact-version library documentation."],
  ["/pro", "pro.md", "Pro reference", "Technical documentation for Lockdocs Pro upgrade reports and private package docs."],
] as const;
export const docPages = entries.map(([route, source, title, description]) => ({ route, source, title, description }));
export type DocPage = (typeof docPages)[number];
const sourceRoutes = new Map<string, string>(docPages.map(({source, route}) => [source, route]));
export const docGroups = [
  { label: "Introduction", pages: docPages.slice(0, 1) },
  { label: "Guide", pages: docPages.slice(1, 6) },
  { label: "Reference", pages: docPages.slice(6, 8) },
  { label: "Project", pages: docPages.slice(8) },
];

export function docHref(href: string, page: DocPage) {
  if (href.startsWith("#") || /^[a-z][a-z0-9+.-]*:/i.test(href)) return href;
  const [pathname, hash = ""] = href.split("#", 2);
  const file = (pathname.startsWith("/") ? pathname.slice(1) : new URL(pathname, "https://docs.invalid/" + page.source).pathname.slice(1)).replace(/\.md$/, "") + ".md";
  return (sourceRoutes.get(file) ?? href.split("#", 1)[0]) + (hash ? "#" + hash : "");
}

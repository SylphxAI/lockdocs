---
layout: home
hero:
  name: lockdocs
  text: The docs for the version you actually installed.
  tagline: Context7 without rate limits. lockdocs reads your lockfile and answers your AI agent from the installed packages' own docs and type declarations. npm, PyPI, crates.io, Go. Offline. No API key. MIT.
  actions:
    - theme: brand
      text: npx -y @sylphx/lockdocs setup
      link: /guide/quickstart
    - theme: alt
      text: Benchmarks
      link: /benchmarks
    - theme: alt
      text: GitHub
      link: https://github.com/SylphxAI/lockdocs
features:
  - icon: 🔒
    title: Exact version, zero config
    details: Reads package-lock, pnpm, yarn, bun, Cargo.lock, uv, poetry, Pipfile, requirements and go.mod. No library IDs, no version hints in the prompt.
  - icon: 📦
    title: Docs that ship with the code
    details: READMEs, changelogs and docs folders, plus API reference from .d.ts + JSDoc, Python docstrings, rustdoc and Go doc comments. Private packages included.
  - icon: ✈️
    title: Offline and unlimited
    details: Everything comes from node_modules, your virtualenv, the Cargo registry and the Go module cache. No network, no account, no rate limit.
  - icon: 🎯
    title: Small, cited answers
    details: Hybrid search (BM25 + a small local embedding model) over symbols and sections, packed into a 1,200-token budget by default, every section cited as package@version path:line.
  - icon: 🏷️
    title: Upstream docs at the exact tag
    details: Next.js, Django or FastAPI ship no docs? `lockdocs fetch` pulls their docs folders from GitHub at your version's git tag, once.
  - icon: 🧰
    title: Three obvious tools
    details: resolve (which versions), docs (a question, optionally scoped to a package), api (the exact signature of z.object or tokio::spawn).
  - icon: ⚡
    title: Rust, cached per version
    details: Each package version is indexed once, in a fraction of a second, then cached. Warm queries take tens of milliseconds.
---

<div class="hero-shot">
  <img src="/img/demo.gif" alt="lockdocs demo: the same question about zod gets .strict() in a zod 3 project and z.strictObject() in a zod 4 project">
  <p>One question, two projects: zod 3 gets <code>.strict()</code>, zod 4 gets <code>z.strictObject()</code>, each cited to the installed file and line.</p>
</div>

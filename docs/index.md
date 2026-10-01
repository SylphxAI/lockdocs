---
layout: home
title: lockdocs
titleTemplate: ":title — exact-version library docs for AI agents, from your lockfile"
hero:
  name: lockdocs
  text: The docs for the version you actually installed.
  tagline: Exact-version library docs from your lockfile — local, offline, no rate limits.
  image:
    src: /logo.svg
    alt: ""
  actions:
    - theme: brand
      text: Get started
      link: /guide/quickstart
    - theme: alt
      text: Benchmarks
      link: /benchmarks
    - theme: alt
      text: GitHub
      link: https://github.com/SylphxAI/lockdocs
install: npx -y @sylphx/lockdocs setup
proof:
  - value: "96/105"
    label: version-sensitive questions right after lockdocs fetch (Context7 77/105)
    link: /benchmarks
  - value: "70/105"
    label: with default settings from an empty cache
    link: /benchmarks
  - value: "58 ms"
    label: median per question after lockdocs fetch
    link: /benchmarks
media:
  kind: video
  src: /img/demo.mp4
  poster: /img/demo-poster.png
  alt: "lockdocs demo: the same question in a Next.js 14 and a Next.js 15 project, then pydantic 1.10.18 answers BaseModel.dict() and pydantic 2.9.2 answers model_dump(), each cited to the installed file and line"
  caption: "One question, two installs: pydantic 1.10.18 answers BaseModel.dict() (pydantic/main.py:427), pydantic 2.9.2 answers model_dump() (pydantic/main.py:352). It opens with Next.js 14 vs 15 cookies()."
features:
  - title: Exact version, zero config
    details: Reads package-lock, pnpm, yarn, bun, Cargo.lock, uv, poetry, Pipfile, requirements and go.mod. No library IDs, no version hints in the prompt.
  - title: Docs that ship with the code
    details: READMEs, changelogs and docs folders, plus API reference from .d.ts + JSDoc, Python docstrings, rustdoc and Go doc comments. Private packages included.
  - title: Upstream docs on first use
    details: Next.js, Django or FastAPI ship no docs in the package? The first query adds their public GitHub docs at the commit your pinned release tag resolves to, anonymously and once. <code>--no-fetch</code> turns it off.
  - title: Offline after the first fetch
    details: Package files come from node_modules, your virtualenv, the Cargo registry and the Go module cache. Once the model and first-use docs are cached it runs offline, and <code>--offline</code> blocks every download. No account, no quota.
  - title: Small, cited answers, fast
    details: Hybrid search (BM25 + a small local embedding model) packs each answer into a 1,200-token budget, every section cited as package@version path:line. Median 58 ms per question after <code>lockdocs fetch</code>.
  - title: Three obvious tools
    details: resolve (which versions), docs (a question, optionally scoped to a package), api (the exact signature of z.object or tokio::spawn).
---

import { defineConfig } from 'vitepress'

const url = 'https://sylphxai.github.io/lockdocs/'
const desc = 'Exact-version library docs from your lockfile — local, offline, no rate limits.'
const icon = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect x='6' y='14' width='20' height='14' rx='3' fill='%237c9cff'/%3E%3Cpath d='M10 14V10a6 6 0 0 1 12 0v4' fill='none' stroke='%2342d6a4' stroke-width='3'/%3E%3Crect x='11' y='18' width='10' height='2' rx='1' fill='%2306080c'/%3E%3Crect x='11' y='22' width='7' height='2' rx='1' fill='%2306080c'/%3E%3C/svg%3E"

export default defineConfig({
  base: '/lockdocs/',
  title: 'lockdocs',
  description: desc,
  appearance: 'force-dark',
  cleanUrls: true,
  // Product vision and capability table are for maintainers, not site pages.
  srcExclude: ['vision.md', 'capabilities.md'],
  lastUpdated: true,
  sitemap: { hostname: url },
  head: [
    ['link', { rel: 'icon', href: icon }],
    ['meta', { name: 'theme-color', content: '#06080c' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:site_name', content: 'lockdocs' }],
    ['meta', { property: 'og:image', content: `${url}img/demo.gif` }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
  ],
  // Each page names its own URL, so search engines index every page, not just the home page.
  transformPageData(pageData) {
    const pageUrl = url + pageData.relativePath.replace(/(^|\/)index\.md$/, '$1').replace(/\.md$/, '')
    const pageTitle = pageData.frontmatter.title ?? (pageData.title || 'lockdocs')
    const pageDesc = pageData.frontmatter.description ?? desc
    pageData.frontmatter.head ??= []
    pageData.frontmatter.head.push(
      ['link', { rel: 'canonical', href: pageUrl }],
      ['meta', { property: 'og:url', content: pageUrl }],
      ['meta', { property: 'og:title', content: pageTitle }],
      ['meta', { property: 'og:description', content: pageDesc }],
    )
  },
  themeConfig: {
    logo: { src: '/logo.svg', alt: 'lockdocs' },
    nav: [
      { text: 'Quickstart', link: '/guide/quickstart' },
      { text: 'Tools', link: '/reference/tools' },
      { text: 'Benchmarks', link: '/benchmarks' },
      { text: 'Compare', link: '/compare' },
      { text: 'npm', link: 'https://www.npmjs.com/package/@sylphx/lockdocs' },
    ],
    sidebar: [
      { text: 'Guide', items: [
        { text: 'Quickstart', link: '/guide/quickstart' },
        { text: 'Editors and agents', link: '/guide/setup' },
        { text: 'Ecosystems', link: '/guide/ecosystems' },
        { text: 'Upstream docs and fetching', link: '/guide/fetch' },
        { text: 'How it works', link: '/guide/how-it-works' },
      ] },
      { text: 'Reference', items: [
        { text: 'MCP tools', link: '/reference/tools' },
        { text: 'CLI', link: '/reference/cli' },
      ] },
      { text: 'More', items: [
        { text: 'Benchmarks', link: '/benchmarks' },
        { text: 'Comparison', link: '/compare' },
      ] },
    ],
    socialLinks: [{ icon: 'github', link: 'https://github.com/SylphxAI/lockdocs' }],
    editLink: { pattern: 'https://github.com/SylphxAI/lockdocs/edit/main/docs/:path' },
    search: { provider: 'local' },
    footer: { message: 'MIT licensed · local, offline, no API key', copyright: '© Sylphx' },
  },
})

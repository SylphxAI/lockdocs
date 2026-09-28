import { defineConfig } from 'vitepress'
import tokens from '../../brand/tokens.json'

const base = '/lockdocs/'
const url = 'https://sylphxai.github.io/lockdocs/'
const desc = 'Exact-version library docs from your lockfile — local, offline, no rate limits.'

export default defineConfig({
  base,
  title: 'lockdocs',
  description: desc,
  appearance: 'force-dark',
  cleanUrls: true,
  // Product vision and capability table are for maintainers, not site pages.
  srcExclude: ['vision.md', 'capabilities.md'],
  lastUpdated: true,
  sitemap: { hostname: url },
  head: [
    // The icons are copies of the brand home's files (brand/svg, brand/favicon).
    ['link', { rel: 'icon', href: `${base}favicon.ico`, sizes: '48x48' }],
    ['link', { rel: 'icon', type: 'image/svg+xml', href: `${base}favicon.svg` }],
    ['meta', { name: 'theme-color', content: tokens.color.bg.$value }],
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

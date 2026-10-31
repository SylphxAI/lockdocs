import { h, onMounted, onUnmounted } from 'vue'
import { installFencedCodeCopy } from './fenced-code-copy'
import './fenced-code-copy.css'
import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import SylphxFooter from './SylphxFooter.vue'
import HeroExtras from './HeroExtras.vue'
import HomeMedia from './HomeMedia.vue'
import ProNavLink from './ProNavLink.vue'
import './custom.css'
import './sylphx.css'

export default {
  extends: DefaultTheme,
  setup() {
    let dispose: (() => void) | undefined
    onMounted(() => {
      dispose = installFencedCodeCopy()
    })
    onUnmounted(() => dispose?.())
  },
  Layout: () =>
    h(DefaultTheme.Layout, null, {
      'nav-bar-content-after': () => h(ProNavLink),
      'home-hero-actions-after': () => h(HeroExtras),
      'home-features-before': () => h(HomeMedia),
      'doc-after': () => h(SylphxFooter, { where: 'doc' }),
      'layout-bottom': () => h(SylphxFooter, { where: 'page' }),
    }),
} satisfies Theme

<script setup lang="ts">
// Home hero: a copyable install command and a row of measured proof points.
// Reads frontmatter.install (string) and frontmatter.proof ({ value, label, link }[]).
import { nextTick, onBeforeUnmount, ref } from 'vue'
import { useData, withBase } from 'vitepress'

const { frontmatter } = useData()
const state = ref<'idle' | 'copying' | 'copied' | 'failed'>('idle')
const manual = ref<HTMLTextAreaElement | null>(null)
let timer: ReturnType<typeof setTimeout> | undefined
onBeforeUnmount(() => clearTimeout(timer))
async function copy() {
  if (state.value === 'copying') return
  clearTimeout(timer)
  state.value = 'copying'
  try {
    await navigator.clipboard.writeText(frontmatter.value.install)
    state.value = 'copied'
    timer = setTimeout(() => (state.value = 'idle'), 1600)
  } catch {
    state.value = 'failed'
    await nextTick()
    manual.value?.focus()
    manual.value?.select()
  }
}
const href = (link: string) => (link.startsWith('/') ? withBase(link) : link)
</script>

<template>
  <div v-if="frontmatter.install || frontmatter.proof" class="sx-hero-extras">
    <div v-if="frontmatter.install" class="sx-install">
      <code tabindex="0"><span class="sx-install__prompt" aria-hidden="true">$</span> {{ frontmatter.install }}</code>
      <button type="button" class="sx-install__copy" :disabled="state === 'copying'" :aria-label="state === 'copied' ? 'Copied' : state === 'failed' ? 'Retry copy install command' : 'Copy install command'" @click="copy">
        {{ state === 'copied' ? 'Copied' : state === 'copying' ? 'Copying…' : state === 'failed' ? 'Retry copy' : 'Copy' }}
      </button>
      <span class="sx-visually-hidden" aria-live="polite">{{ state === 'copied' ? 'Install command copied' : '' }}</span>
    </div>
    <div v-if="state === 'failed'" class="sx-copy-recovery">
      <p role="status">Copy failed. Copy the selected command manually, or retry.</p>
      <textarea ref="manual" readonly :value="frontmatter.install" aria-label="Install command to copy manually" rows="2" @focus="manual?.select()" />
    </div>
    <ul v-if="frontmatter.proof" class="sx-proof">
      <li v-for="p in frontmatter.proof" :key="p.value">
        <a :href="href(p.link)">
          <strong>{{ p.value }}</strong>
          <span>{{ p.label }}</span>
        </a>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.sx-copy-recovery { margin-top: 12px; font-size: 14px; }
.sx-copy-recovery textarea { display: block; width: 100%; margin-top: 8px; padding: 10px; border: 1px solid var(--vp-c-brand-1); border-radius: 6px; background: var(--vp-c-bg-soft); font: 13px var(--vp-font-family-mono); resize: vertical; }
</style>

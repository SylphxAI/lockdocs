<script setup lang="ts">
import { withBase } from 'vitepress'
</script>

<template>
  <!-- Keep pricing one link away even when the main menu is collapsed.
       Keel owns this page, so use a full navigation rather than the docs router. -->
  <a class="pro-nav-link" :href="withBase('/pricing')" target="_self">Pricing</a>
</template>

<style scoped>
.pro-nav-link {
  display: inline-flex;
  align-items: center;
  min-height: 44px;
  padding: 0 4px;
  color: var(--vp-c-brand-1);
  font-size: 14px;
  font-weight: 500;
}

.pro-nav-link:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 2px;
}

@media (min-width: 768px) {
  .pro-nav-link { display: none; }
}
</style>

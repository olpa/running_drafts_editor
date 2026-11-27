<script setup lang="ts">
declare const __RUNNING_DRAFTS_POLICY_LINKS__: ReadonlyArray<{ label: string; href: string }>;
const policyLinks = __RUNNING_DRAFTS_POLICY_LINKS__;
</script>

<template>
  <footer v-if="policyLinks.length" class="legal-footer">
    <nav aria-label="Legal">
      <a v-for="policy in policyLinks" :key="policy.href" :href="policy.href">{{ policy.label }}</a>
    </nav>
  </footer>
</template>

<style scoped>
.legal-footer {
  margin-inline: auto;
}
nav {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem 1.25rem;
}
</style>

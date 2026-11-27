<script setup lang="ts">
import SessionPanel from "./SessionPanel.vue";
import UsageSummary from "../usage/UsageSummary.vue";
</script>

<template>
  <section class="account-page" data-testid="account-page" aria-labelledby="account-title">
    <h1 id="account-title" tabindex="-1">Account</h1>
    <SessionPanel />
    <UsageSummary />
  </section>
</template>

<style scoped>
.account-page {
  max-width: 48rem;
}
h1 {
  margin-bottom: 1rem;
}
</style>

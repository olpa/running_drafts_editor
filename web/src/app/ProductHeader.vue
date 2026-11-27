<template>
  <header class="product-header">
    <strong>Running Drafts</strong>
    <span v-if="$slots.default"><slot /></span>
  </header>
</template>

<style scoped>
.product-header {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.5rem 1rem;
}
strong {
  font-size: 1.25rem;
}
span {
  color: #4c6054;
  font-size: 0.875rem;
}
</style>

<template>
  <header class="product-header">
    <strong><slot name="brand">Running Drafts</slot></strong>
    <span v-if="$slots.default"><slot /></span>
    <slot name="navigation" />
  </header>
</template>

<style scoped>
.product-header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.5rem 1rem;
}
strong {
  font-size: 1.25rem;
}
strong :deep(a) {
  color: inherit;
  text-decoration: none;
}
strong :deep(a:hover) {
  text-decoration: underline;
}
span {
  color: #4c6054;
  font-size: 0.875rem;
}
</style>

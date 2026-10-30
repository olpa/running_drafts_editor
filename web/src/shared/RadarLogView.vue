<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  lines: readonly string[];
  cursor: number;
  columns: number;
  label: string;
  testId?: string;
}>();

const parts = computed(() => {
  const text = props.lines.join("\n");
  return {
    before: text.slice(0, props.cursor),
    mark: text.charAt(props.cursor),
    after: text.slice(props.cursor + 1),
  };
});
</script>

<template>
  <!-- prettier-ignore -->
  <pre
    class="radar-log"
    role="region"
    :data-testid="testId"
    :aria-label="label"
    :style="{ '--log-columns': columns }"
  >{{ parts.before }}<span class="cursor">{{ parts.mark }}</span>{{ parts.after }}</pre>
</template>

<style scoped>
.radar-log {
  width: min(100%, calc(var(--log-columns) * 1ch + 1rem));
  overflow-x: auto;
  padding: 0.75rem;
  border-radius: 6px;
  background: #111827;
  color: #86efac;
  line-height: 1.2;
}
.cursor {
  background: #86efac;
  color: #111827;
}
</style>

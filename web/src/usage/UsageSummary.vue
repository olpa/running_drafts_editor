<script setup lang="ts">
import { computed } from "vue";
import AllowanceStatus from "../recording/AllowanceStatus.vue";
import { useUsage } from "./usage.ts";

const usage = useUsage(true);
const snapshot = computed(() => usage?.snapshot.value ?? null);

function bytes(value: number): string {
  const units = ["bytes", "KiB", "MiB", "GiB"];
  let amount = value;
  let unit = 0;
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024;
    unit++;
  }
  const digits = unit === 0 || amount >= 10 ? 0 : 1;
  return `${amount.toFixed(digits)} ${units[unit]}`;
}

function duration(value: number): string {
  const minutes = value / 60_000;
  if (minutes < 60) {
    const rounded = Math.round(minutes);
    return `${rounded} ${rounded === 1 ? "minute" : "minutes"}`;
  }
  const hours = minutes / 60;
  return `${Number.isInteger(hours) ? hours : hours.toFixed(1)} hours`;
}
</script>

<template>
  <section
    v-if="usage"
    class="usage-summary"
    data-testid="usage-summary"
    aria-labelledby="usage-summary-title"
    aria-live="polite"
  >
    <h3 id="usage-summary-title">Usage</h3>
    <AllowanceStatus />
    <p v-if="usage.status.value === 'loading' && !snapshot">Loading usage…</p>
    <p v-else-if="usage.problem.value && !snapshot" role="status">{{ usage.problem.value }}</p>
    <dl v-if="snapshot">
      <div>
        <dt>Plan</dt>
        <dd>{{ snapshot.plan === "free" ? "Free" : "Pro" }}</dd>
      </div>
      <div>
        <dt>Stored audio</dt>
        <dd>
          {{ bytes(snapshot.audioStorage.usedBytes) }} used · warning at
          {{ bytes(snapshot.audioStorage.warningAtBytes) }} · limit
          {{ bytes(snapshot.audioStorage.limitBytes) }}
        </dd>
      </div>
      <div v-if="snapshot.audioStorage.grantedBytes > 0">
        <dt>Storage grants</dt>
        <dd>{{ bytes(snapshot.audioStorage.grantedBytes) }}</dd>
      </div>
      <div>
        <dt>Recording duration</dt>
        <dd>
          Warning at {{ duration(snapshot.audioAssetDuration.warningAtMs) }} · limit
          {{ duration(snapshot.audioAssetDuration.limitMs) }}
        </dd>
      </div>
      <div v-if="snapshot.audioAssetDuration.grantedMs > 0">
        <dt>Duration grants</dt>
        <dd>{{ duration(snapshot.audioAssetDuration.grantedMs) }}</dd>
      </div>
    </dl>
    <p v-if="snapshot && usage.problem.value" class="usage-refresh-problem" role="status">
      {{ usage.problem.value }} Showing the last available values.
    </p>
  </section>
</template>

<style scoped>
.usage-summary {
  margin-bottom: 1rem;
}
.usage-summary h3 {
  margin-bottom: 0.6rem;
  font-size: 1rem;
}
.usage-summary dl {
  display: grid;
  gap: 0.6rem;
  margin: 0;
}
.usage-summary dl > div {
  display: grid;
  grid-template-columns: minmax(7rem, 10rem) 1fr;
  gap: 0.75rem;
}
.usage-summary dt {
  color: var(--muted);
}
.usage-summary dd {
  margin: 0;
}
.usage-refresh-problem {
  margin: 0.6rem 0 0;
  color: var(--warning);
}
@media (max-width: 560px) {
  .usage-summary dl > div {
    grid-template-columns: 1fr;
    gap: 0;
  }
}
</style>

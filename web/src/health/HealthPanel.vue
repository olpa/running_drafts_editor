<script setup lang="ts">
import type { HealthEndpoint } from "./gateway.ts";
import { useHealth } from "./health.ts";

const health = useHealth();

const cards: {
  endpoint: HealthEndpoint;
  icon: string;
  title: string;
  description: string;
  action: string;
}[] = [
  {
    endpoint: "live",
    icon: "L",
    title: "Service liveness",
    description: "Confirms that the backend process is responding.",
    action: "Check liveness",
  },
  {
    endpoint: "ready",
    icon: "R",
    title: "Database readiness",
    description: "Confirms that the backend and its database are ready.",
    action: "Check readiness",
  },
];
</script>

<template>
  <div class="health-grid">
    <article
      v-for="card in cards"
      :key="card.endpoint"
      class="health-card"
      :data-testid="`${card.endpoint}-card`"
    >
      <div class="health-card-heading">
        <span class="health-icon" aria-hidden="true">{{ card.icon }}</span>
        <div>
          <h3>{{ card.title }}</h3>
          <p>{{ card.description }}</p>
        </div>
      </div>
      <code>/health/{{ card.endpoint }}</code>
      <button
        class="primary"
        type="button"
        :data-testid="`${card.endpoint}-check`"
        :disabled="health.checks[card.endpoint].state === 'busy'"
        @click="health.check(card.endpoint)"
      >
        {{ card.action }}
      </button>
      <div
        class="health-result"
        :data-testid="`${card.endpoint}-result`"
        :data-state="
          health.checks[card.endpoint].state === 'idle'
            ? undefined
            : health.checks[card.endpoint].state
        "
        role="status"
        aria-live="polite"
      >
        <span class="status-dot" aria-hidden="true"></span>
        <div>
          <strong :data-testid="`${card.endpoint}-summary`">{{
            health.checks[card.endpoint].summary
          }}</strong>
          <p :data-testid="`${card.endpoint}-detail`">{{ health.checks[card.endpoint].detail }}</p>
        </div>
      </div>
    </article>
  </div>
</template>

<style scoped>
.health-grid {
  display: grid;
  gap: 16px;
}
.health-card {
  padding: 20px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
  box-shadow: var(--shadow);
}
.health-card-heading {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}
.health-card-heading p {
  margin-bottom: 0;
  color: var(--muted);
  font-size: 0.9rem;
  line-height: 1.5;
}
.health-icon {
  display: grid;
  flex: 0 0 2.15rem;
  width: 2.15rem;
  height: 2.15rem;
  place-items: center;
  border-radius: 7px;
  background: #e8f1ff;
  color: #0f4c9f;
  font-weight: 800;
}
.health-card code {
  display: block;
  margin: 16px 0 12px;
  color: var(--muted);
  font-size: 0.85rem;
}
.health-card .primary {
  width: 100%;
}
.health-result {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  min-height: 4rem;
  margin-top: 14px;
  padding: 11px 12px;
  border-left: 4px solid var(--line);
  background: #f8fafc;
}
.health-result strong {
  display: block;
  font-size: 0.92rem;
}
.health-result p {
  margin: 3px 0 0;
  color: var(--muted);
  overflow-wrap: anywhere;
  font-size: 0.82rem;
}
.status-dot {
  flex: 0 0 0.55rem;
  height: 0.55rem;
  margin-top: 0.35rem;
  border-radius: 50%;
  background: var(--line);
}
.health-result[data-state="busy"] {
  border-color: var(--accent);
}
.health-result[data-state="busy"] .status-dot {
  background: var(--accent);
}
.health-result[data-state="success"] {
  border-color: var(--success);
  background: #f0f9f4;
}
.health-result[data-state="success"] strong {
  color: var(--success);
}
.health-result[data-state="success"] .status-dot {
  background: var(--success);
}
.health-result[data-state="error"] {
  border-color: var(--danger);
  background: #fff7f7;
}
.health-result[data-state="error"] strong {
  color: var(--danger);
}
.health-result[data-state="error"] .status-dot {
  background: var(--danger);
}
</style>

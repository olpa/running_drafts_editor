<script setup lang="ts">
import HealthPanel from "../health/HealthPanel.vue";
import SeedApp from "./SeedApp.vue";
</script>

<template>
  <main class="app-shell">
    <header class="topbar">
      <div>
        <h1>Running Drafts</h1>
        <p>Development tools</p>
      </div>
      <span class="environment-badge">Development</span>
    </header>

    <div class="workspace">
      <section class="workspace-pane health-surface" aria-labelledby="health-title">
        <header class="section-heading">
          <p class="eyebrow">Environment</p>
          <h2 id="health-title">Backend health</h2>
          <p>Check whether the service is running and ready to use its database.</p>
        </header>
        <HealthPanel />
      </section>

      <section class="workspace-pane seed-surface" aria-labelledby="seed-title">
        <header class="section-heading">
          <p class="eyebrow">Browser seed</p>
          <h2 id="seed-title">Recording experiment</h2>
          <p>The existing microphone, delivery, and transcription test surface.</p>
        </header>
        <SeedApp />
      </section>
    </div>
  </main>
</template>

<style scoped>
.app-shell {
  min-height: 100vh;
  padding: 24px;
}
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  width: min(1440px, 100%);
  margin: 0 auto 18px;
}
.topbar p {
  margin-bottom: 0;
  color: var(--muted);
  font-size: 0.9rem;
  line-height: 1.5;
}
.environment-badge {
  padding: 0.35rem 0.65rem;
  border: 1px solid #bfd7ff;
  border-radius: 999px;
  background: #e8f1ff;
  color: #0f4c9f;
  font-size: 0.82rem;
  font-weight: 700;
}
.workspace {
  display: grid;
  grid-template-columns: minmax(320px, 0.8fr) minmax(500px, 1.2fr);
  width: min(1440px, 100%);
  min-height: calc(100vh - 105px);
  margin: 0 auto;
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
  box-shadow: var(--shadow);
}
.workspace-pane {
  min-width: 0;
  padding: 28px;
}
.workspace-pane + .workspace-pane {
  border-left: 1px solid var(--line);
}
.health-surface {
  background: #f8fafc;
}
.section-heading {
  margin-bottom: 24px;
}
.section-heading p {
  color: var(--muted);
  line-height: 1.5;
}
.section-heading > p:last-child {
  max-width: 46rem;
  margin-bottom: 0;
}
.eyebrow {
  margin-bottom: 6px;
  color: var(--accent-dark) !important;
  font-size: 0.75rem;
  font-weight: 750;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
@media (max-width: 900px) {
  .app-shell {
    padding: 16px;
  }
  .workspace {
    grid-template-columns: 1fr;
  }
  .workspace-pane + .workspace-pane {
    border-top: 1px solid var(--line);
    border-left: 0;
  }
}
@media (max-width: 560px) {
  .app-shell {
    padding: 12px;
  }
  .workspace-pane {
    padding: 20px 16px;
  }
  .topbar {
    margin-bottom: 12px;
  }
}
</style>

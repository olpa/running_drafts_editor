<script setup lang="ts">
import { computed, ref, useId, watch } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { useSession } from "../session/session.ts";

const route = useRoute();
const session = useSession();
const open = ref(false);
const trigger = ref<globalThis.HTMLButtonElement | null>(null);
const menuId = useId();
const initials = computed(() => {
  const state = session.state.value;
  if (state.status !== "signedIn") return "A";

  const words = state.session.profile.displayName?.trim().split(/\s+/u).filter(Boolean) ?? [];
  const first = Array.from(words[0] ?? "")[0];
  const last = words.length > 1 ? Array.from(words.at(-1) ?? "")[0] : "";
  return first ? (first + (last ?? "")).toLocaleUpperCase() : "A";
});

function closeMenu(restoreFocus = false): void {
  if (!open.value) return;
  open.value = false;
  if (restoreFocus) trigger.value?.focus({ preventScroll: true });
}

function leaveMenu(event: globalThis.FocusEvent): void {
  if (
    event.currentTarget instanceof globalThis.HTMLElement &&
    event.relatedTarget instanceof globalThis.Node &&
    event.currentTarget.contains(event.relatedTarget)
  ) {
    return;
  }
  closeMenu();
}

function logout(): void {
  closeMenu(true);
  void session.logout();
}

watch(
  () => route.fullPath,
  () => closeMenu(),
);
</script>

<template>
  <div class="account-menu" @focusout="leaveMenu" @keydown.esc.stop="closeMenu(true)">
    <button
      ref="trigger"
      type="button"
      class="account-trigger"
      aria-label="Account menu"
      :aria-expanded="open"
      :aria-controls="menuId"
      @click="open = !open"
    >
      <span aria-hidden="true">{{ initials }}</span>
    </button>
    <nav v-show="open" :id="menuId" class="menu-panel" aria-label="Main">
      <RouterLink
        :to="{ name: 'projects', query: route.query, hash: route.hash }"
        @click="closeMenu(true)"
      >
        Projects
      </RouterLink>
      <RouterLink
        :to="{ name: 'account', query: route.query, hash: route.hash }"
        @click="closeMenu(true)"
      >
        Account
      </RouterLink>
      <div class="sign-out">
        <button type="button" @click="logout">Log out</button>
      </div>
    </nav>
  </div>
</template>

<style scoped>
.account-menu {
  position: relative;
  margin-inline-start: auto;
}
.account-trigger {
  display: grid;
  place-items: center;
  min-inline-size: 44px;
  min-block-size: 44px;
  padding: 0.25rem;
  border-radius: 50%;
  background: #e8f1ff;
  color: var(--accent-dark);
}
.account-trigger:hover,
.account-trigger[aria-expanded="true"] {
  border-color: var(--accent-dark);
}
.menu-panel {
  position: absolute;
  z-index: 2;
  inset-block-start: calc(100% + 0.5rem);
  inset-inline-end: 0;
  display: grid;
  min-inline-size: 10rem;
  padding: 0.25rem;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
  box-shadow: var(--shadow);
}
.menu-panel a,
.sign-out button {
  display: flex;
  align-items: center;
  min-height: 44px;
  padding: 0.5rem 0.75rem;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--text);
  font-weight: 550;
  text-decoration: none;
}
.menu-panel a:hover,
.menu-panel a[aria-current="page"],
.sign-out button:hover {
  background: #e8f1ff;
  color: var(--accent-dark);
}
.sign-out {
  margin-block-start: 0.25rem;
  padding-block-start: 0.25rem;
  border-block-start: 1px solid var(--line);
}
.sign-out button {
  inline-size: 100%;
}
</style>

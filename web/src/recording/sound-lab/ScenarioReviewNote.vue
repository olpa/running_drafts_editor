<script setup lang="ts">
defineProps<{ initial: string; play: string; inspect: string; interaction: string }>();
</script>

<template>
  <section class="review-note" aria-labelledby="review-note-heading" data-review-note>
    <h2 id="review-note-heading">Review note</h2>
    <dl>
      <div data-review-field="initial">
        <dt>Initial condition</dt>
        <dd>{{ initial }}</dd>
      </div>
      <div data-review-field="play">
        <dt>Play scenario</dt>
        <dd>{{ play }}</dd>
      </div>
      <div data-review-field="inspect">
        <dt>Inspect</dt>
        <dd>{{ inspect }}</dd>
      </div>
      <div data-review-field="interaction">
        <dt>Interaction</dt>
        <dd>{{ interaction }}</dd>
      </div>
    </dl>
  </section>
</template>

<style scoped>
.review-note {
  padding: 1rem;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: #f8fafc;
}
.review-note h2 {
  margin-top: 0;
}
.review-note dl {
  display: grid;
  gap: 0.6rem;
  margin: 0;
}
.review-note dl div {
  display: grid;
  grid-template-columns: 10rem 1fr;
  gap: 0.75rem;
}
.review-note dt {
  font-weight: 700;
}
.review-note dd {
  margin: 0;
}
</style>

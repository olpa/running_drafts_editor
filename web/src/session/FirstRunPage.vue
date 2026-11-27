<script setup lang="ts">
import { computed } from "vue";
import LegalFooter from "../app/LegalFooter.vue";
import ProductHeader from "../app/ProductHeader.vue";
import landingIntro from "./landing-intro.html?raw";
import recordingWorkflow from "./recording-workflow-variant.png";
import { useSession } from "./session.ts";

const session = useSession();
const state = computed(() => session.state.value);
const [introduction, landingDetails] = landingIntro.split("<!-- audio-saving -->");
const [audioSaving, signInCopy] = landingDetails!.split("<!-- sign-in -->");
</script>

<template>
  <div v-if="state.status !== 'signedIn'" class="landing">
    <ProductHeader class="brand">Early preview</ProductHeader>
    <main class="intro" aria-labelledby="landing-title">
      <!-- The markup is a bundled, product-owned static source. -->
      <!-- eslint-disable-next-line vue/no-v-html -->
      <div class="landing-copy" v-html="introduction"></div>
      <figure class="recording-workflow">
        <img
          :src="recordingWorkflow"
          width="1536"
          height="1024"
          alt="Four panels show a person speaking into a clip-on mic connected to a pocketed phone, Running Drafts saving audio as they speak, an AI agent receiving saved audio through the Running Drafts API, and the person finishing the outline."
        />
        <figcaption>AI-generated</figcaption>
      </figure>
      <!-- eslint-disable-next-line vue/no-v-html -->
      <div class="landing-copy" v-html="audioSaving"></div>
      <div class="sign-in" data-testid="authentication">
        <!-- eslint-disable-next-line vue/no-v-html -->
        <div class="landing-copy" v-html="signInCopy"></div>
        <div aria-live="polite">
          <p v-if="state.status === 'loading'">Checking your session…</p>
          <template v-else>
            <p v-if="state.message !== 'Sign in to continue.'">{{ state.message }}</p>
            <p v-if="state.status === 'signedOut' && state.signInError">
              Error code: {{ state.signInError }}
            </p>
            <p v-if="state.referenceId">Request ID: {{ state.referenceId }}</p>
          </template>
        </div>
        <button v-if="state.status === 'failure'" class="primary" @click="session.bootstrap()">
          Try again
        </button>
        <template v-else-if="state.status === 'signedOut'">
          <button v-if="state.loginAllowed" class="primary" @click="session.login('google')">
            Sign in with Google
          </button>
          <button v-if="state.logoutUnconfirmed" @click="session.logout()">Retry logout</button>
        </template>
      </div>
    </main>
    <LegalFooter class="policies" />
  </div>
</template>

<style scoped>
.landing {
  min-height: 100vh;
  padding: clamp(1.25rem, 5vw, 4rem);
  background: #f6f4ed;
  color: #202e27;
}
.brand,
.intro,
.policies {
  width: min(100%, 52rem);
  margin-inline: auto;
}
.intro {
  padding-block: clamp(2.75rem, 10vw, 7rem) 2.5rem;
}
.landing-copy :deep(h1) {
  max-width: 14ch;
  font-family: Georgia, serif;
  font-size: clamp(2.55rem, 7vw, 5.25rem);
  line-height: 1.04;
}
.landing-copy :deep(p) {
  max-width: 36rem;
  line-height: 1.55;
}
.landing-copy :deep(.summary) {
  margin-block: 1rem 1.5rem;
  font-size: clamp(1.125rem, 3vw, 1.5rem);
}
.recording-workflow {
  position: relative;
  margin: 1.5rem 0 1rem;
}
/* Keep the original responsive image sizing unless a resize is explicitly requested. */
.recording-workflow img {
  display: block;
  width: 100%;
  height: auto;
  border-radius: 8px;
}
.recording-workflow figcaption {
  position: absolute;
  bottom: 0.45rem;
  left: 0.45rem;
  padding: 0.2rem 0.45rem;
  border-radius: 4px;
  background: #202e27;
  color: #fff;
  font-size: 0.7rem;
}
.sign-in {
  max-width: 36rem;
  margin-top: 1.5rem;
  padding: 1rem 1.25rem;
  border: 1px solid #becdc1;
  border-radius: 8px;
  background: #edf2eb;
}
button {
  min-height: 48px;
}
button.primary {
  background: #244c39;
  border-color: #244c39;
  color: #fff;
}
.policies :deep(a) {
  color: #244c39;
}
button:focus-visible,
.policies :deep(a):focus-visible {
  outline: 3px solid #244c39;
  outline-offset: 4px;
}
@media (min-width: 48rem) {
  .landing {
    padding-top: 2rem;
  }
  .intro {
    padding-top: 2rem;
  }
  .landing-copy :deep(h1) {
    max-width: none;
    font-size: 3.75rem;
  }
}
</style>

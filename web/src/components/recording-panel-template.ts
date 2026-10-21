const template = `<section aria-labelledby="project-recording-title">
  <header class="toolbar">
    <h2 id="project-recording-title" data-testid="project-title">New Project</h2>
    <button type="button" class="primary primary-control" data-testid="start" data-mode="start">Start</button>
    <span data-testid="elapsed" aria-label="Elapsed capture time">0:00</span>
  </header>
  <div class="capture-indicator" data-testid="capture-indicator" data-capturing="false" role="status">
    <span class="capture-dot" aria-hidden="true"></span>
    <span data-testid="capture-message">Press Start when you are ready to capture audio.</span>
  </div>
  <div class="capture-progress" data-testid="capture-progress" hidden>
    <span data-testid="capture-time" aria-live="off"></span>
    <progress data-testid="capture-duration" aria-label="Capture duration"></progress>
  </div>
  <p data-testid="start-problem" role="status" aria-live="polite"></p>
  <p class="ownership-warning" data-testid="ownership-warning" role="status" aria-live="polite"></p>
  <ol class="section-deliveries" data-testid="sections" aria-label="Recording sections"></ol>
</section>`;

export default template;

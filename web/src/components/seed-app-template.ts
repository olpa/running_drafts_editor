const template = `<section class="authentication" data-testid="authentication" aria-live="polite"></section>
<nav class="private-navigation" data-testid="private-navigation" aria-label="Private workspace" hidden>
  <button type="button" data-testid="record-view">Record</button>
  <button type="button" data-testid="projects-view">Projects</button>
</nav>
<div data-testid="recording-workspace" hidden>
  <div data-testid="recording-panel-host"></div>
  <div data-testid="private-results">
    <ol class="transcript" data-testid="transcript"></ol>
    <pre class="radar-log" data-testid="log" aria-label="Diagnostic log"></pre>
  </div>
</div>
<div data-testid="projects-workspace" hidden>
  <project-browser></project-browser>
</div>`;

export default template;

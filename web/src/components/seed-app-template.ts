const template = `<section class="authentication" data-testid="authentication" aria-live="polite"></section>
<div data-testid="recording-workspace" hidden>
  <div data-testid="recording-panel-host"></div>
  <div data-testid="private-results">
    <ol class="transcript" data-testid="transcript"></ol>
    <pre class="radar-log" data-testid="log" aria-label="Diagnostic log"></pre>
  </div>
</div>`;

export default template;

import type { CompletionStatus, SectionSnapshot } from "../recording.ts";

export interface SectionComponentActions {
  uploadPendingAudio(sectionId: string): void;
  retryClose(sectionId: string): void;
}

/** Persistent DOM for one domain Section. */
export class SectionComponent {
  readonly element = document.createElement("section");
  private readonly heading = document.createElement("h3");
  private readonly pending = status();
  private readonly completion = status();
  private readonly problem = status("error");
  private readonly upload = action("Upload pending audio");
  private readonly retryCompletion = action("Try again");
  private readonly closeProblem = status("error");
  private readonly retryClose = action("Could not close recording — Try again");
  private readonly sectionId: string;
  private completionStatus: CompletionStatus | null = null;

  constructor(snapshot: SectionSnapshot, private readonly actions: SectionComponentActions) {
    this.sectionId = snapshot.sectionId;
    this.element.dataset.testid = "section";
    this.completion.role = "status";
    this.completion.setAttribute("aria-live", "polite");
    this.completion.hidden = false;
    this.element.append(this.heading, this.pending, this.completion, this.problem, this.upload,
      this.retryCompletion, this.closeProblem, this.retryClose);
    this.upload.addEventListener("click", this.uploadPendingAudio);
    this.retryCompletion.addEventListener("click", this.retrySectionClose);
    this.retryClose.addEventListener("click", this.retrySectionClose);
    this.update(snapshot);
  }

  update(snapshot: SectionSnapshot): void {
    const sectionNumber = snapshot.position + 1;
    this.heading.textContent = `Section ${sectionNumber}`;
    setText(this.pending, snapshot.pendingMs > 0
      ? `${formatSeconds(snapshot.pendingMs)} of audio pending` : "");
    setText(this.problem, snapshot.problem);
    setVisible(this.upload, snapshot.canUploadPending);

    if (snapshot.completion !== this.completionStatus) {
      this.completionStatus = snapshot.completion;
      this.renderCompletion(snapshot);
    } else if (snapshot.completion === "failed"
      && this.completion.textContent !== snapshot.completionProblem) {
      this.renderCompletion(snapshot);
    }
    setVisible(this.retryCompletion, snapshot.canRetryCompletion);
    this.retryCompletion.setAttribute("aria-label", `Try completing Section ${sectionNumber} again`);

    setText(this.closeProblem, snapshot.closeProblem && !snapshot.canRetryClose
      ? "Could not close recording" : "");
    setVisible(this.retryClose, !!snapshot.closeProblem && snapshot.canRetryClose);
  }

  dispose(): void {
    this.upload.removeEventListener("click", this.uploadPendingAudio);
    this.retryCompletion.removeEventListener("click", this.retrySectionClose);
    this.retryClose.removeEventListener("click", this.retrySectionClose);
    this.element.remove();
  }

  private renderCompletion(snapshot: SectionSnapshot): void {
    if (snapshot.completion) this.element.dataset.completion = snapshot.completion;
    else delete this.element.dataset.completion;
    this.completion.className = `section-status ${completionClass(snapshot.completion)}`.trim();
    const text = snapshot.completion === "pending" ? "Completing recording…"
      : snapshot.completion === "succeeded" ? "Recording complete"
        : snapshot.completion === "failed" ? snapshot.completionProblem : "";
    if (this.completion.textContent !== text) this.completion.textContent = text;
  }

  private readonly uploadPendingAudio = (): void => {
    this.actions.uploadPendingAudio(this.sectionId);
  };

  private readonly retrySectionClose = (): void => {
    this.actions.retryClose(this.sectionId);
  };
}

function status(kind = "warning"): HTMLSpanElement {
  const element = document.createElement("span");
  element.className = `section-status ${kind}`;
  element.hidden = true;
  return element;
}

function action(label: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.hidden = true;
  return button;
}

function setText(element: HTMLElement, text: string): void {
  if (element.textContent !== text) element.textContent = text;
  element.hidden = !text;
}

function setVisible(element: HTMLElement, visible: boolean): void {
  element.hidden = !visible;
}

function completionClass(completion: CompletionStatus | null): string {
  if (completion === "succeeded") return "success";
  if (completion === "failed") return "error";
  return "warning";
}

function formatSeconds(ms: number): string {
  const seconds = Math.ceil(ms / 1000);
  return `${seconds} ${seconds === 1 ? "second" : "seconds"}`;
}

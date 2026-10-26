import { ApiClient, type AudioAssetSummary, type ProjectDetail,
  type ProjectSummary, type SectionSummary } from "../api.ts";

/** Authenticated Project list and complete Project-detail browser. */
export class ProjectBrowser extends HTMLElement {
  private readonly api = new ApiClient({
    onAuthenticationExpired: () => this.dispatchEvent(
      new CustomEvent("authentication-expired", { bubbles: true }),
    ),
  });
  private view!: HTMLElement;
  private status!: HTMLElement;
  private requestGeneration = 0;

  connectedCallback(): void {
    if (this.view) return;
    this.className = "project-browser";
    this.status = document.createElement("p");
    this.status.dataset.testid = "project-browser-status";
    this.status.className = "project-browser-status";
    this.status.setAttribute("role", "status");
    this.view = document.createElement("div");
    this.append(this.status, this.view);
  }

  async showList(): Promise<void> {
    const generation = ++this.requestGeneration;
    this.status.textContent = "Loading Projects…";
    try {
      const projects = await this.api.listProjects();
      if (generation !== this.requestGeneration) return;
      this.renderList(projects);
      this.status.textContent = "";
    } catch {
      if (generation !== this.requestGeneration) return;
      this.renderLoadFailure(
        "Projects could not be loaded.", "Retry Projects", () => void this.showList(),
      );
    }
  }

  clear(): void {
    this.requestGeneration++;
    this.status.textContent = "";
    this.view.replaceChildren();
  }

  private renderList(projects: readonly ProjectSummary[]): void {
    const section = document.createElement("section");
    section.dataset.testid = "project-list";
    const heading = document.createElement("h2");
    heading.textContent = "Projects";
    const list = document.createElement("ol");
    list.className = "project-list";
    for (const project of projects) list.append(this.projectEntry(project));
    if (!projects.length) {
      const empty = document.createElement("p");
      empty.textContent = "No Projects yet.";
      section.append(heading, empty);
    } else {
      section.append(heading, list);
    }
    this.view.replaceChildren(section);
  }

  private projectEntry(project: ProjectSummary): HTMLLIElement {
    const item = document.createElement("li");
    item.dataset.testid = "project-list-item";
    item.dataset.projectId = project.id;
    const open = document.createElement("button");
    open.type = "button";
    open.textContent = project.title;
    open.addEventListener("click", () => void this.showProject(project.id));
    const updated = document.createElement("time");
    updated.dateTime = project.updatedAt;
    updated.textContent = formatDate(project.updatedAt);
    item.append(open, updated);
    return item;
  }

  private async showProject(projectId: string): Promise<void> {
    const generation = ++this.requestGeneration;
    this.status.textContent = "Loading Project…";
    try {
      const project = await this.api.getProjectDetail(projectId);
      if (generation !== this.requestGeneration) return;
      this.renderProject(project);
      this.status.textContent = "";
    } catch {
      if (generation !== this.requestGeneration) return;
      this.renderLoadFailure(
        "The Project could not be loaded.", "Back to Projects", () => void this.showList(),
      );
    }
  }

  private renderProject(project: ProjectDetail): void {
    const article = document.createElement("article");
    article.dataset.testid = "project-detail";
    article.dataset.projectId = project.id;
    const back = document.createElement("button");
    back.type = "button";
    back.textContent = "Back to Projects";
    back.addEventListener("click", () => void this.showList());
    const heading = document.createElement("h2");
    heading.textContent = project.title;
    const sections = document.createElement("ol");
    sections.className = "project-sections";
    for (const section of project.document.sections) {
      sections.append(this.sectionEntry(section));
    }
    article.append(back, heading);
    if (project.document.sections.length) {
      article.append(sections);
    } else {
      const empty = document.createElement("p");
      empty.textContent = "This Project has no Sections.";
      article.append(empty);
    }
    this.view.replaceChildren(article);
  }

  private sectionEntry(section: SectionSummary): HTMLLIElement {
    const item = document.createElement("li");
    item.dataset.testid = "project-section";
    item.dataset.sectionId = section.id;
    item.dataset.audioAssetId = section.audioAsset.id;
    item.dataset.sealed = String(section.audioAsset.isSealed);
    item.dataset.sealReason = section.audioAsset.sealReason ?? "open";
    const heading = document.createElement("h3");
    heading.textContent = `Section ${section.position + 1}`;
    const state = document.createElement("p");
    state.className = "project-section-state";
    state.textContent = assetState(section.audioAsset);
    const diagnostics = document.createElement("p");
    diagnostics.className = "project-section-diagnostics";
    diagnostics.textContent = assetDiagnostics(section.audioAsset);
    item.append(heading, state, diagnostics);
    if (section.audioAsset.isSealed) {
      const download = document.createElement("a");
      download.dataset.testid = "audio-download";
      download.className = "download-control";
      download.href = this.api.audioDownloadUrl(section.audioAsset.id);
      download.textContent = section.audioAsset.sealReason === "interrupted"
        ? "Download interrupted original audio" : "Download original audio";
      item.append(download);
    }
    return item;
  }

  private renderLoadFailure(message: string, label: string, action: () => void): void {
    this.status.textContent = message;
    const control = document.createElement("button");
    control.type = "button";
    control.textContent = label;
    control.addEventListener("click", action);
    this.view.replaceChildren(control);
  }
}

function assetState(asset: AudioAssetSummary): string {
  if (!asset.isSealed) return "Recording is still open and cannot be downloaded.";
  return asset.sealReason === "interrupted"
    ? "Interrupted audio is sealed and available to download."
    : "Completed audio is sealed and available to download.";
}

function assetDiagnostics(asset: AudioAssetSummary): string {
  const duration = (asset.observedDurationMs / 1_000).toFixed(1);
  const sealed = asset.sealedAt ? ` · sealed ${formatDate(asset.sealedAt)}` : "";
  return `${asset.frameCount} frames · ${asset.storedBytes} bytes · ${duration} seconds${sealed}`;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium", timeStyle: "short",
  }).format(date);
}

customElements.define("project-browser", ProjectBrowser);

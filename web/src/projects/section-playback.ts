import { computed, shallowRef, type Ref } from "vue";
import { ApiError } from "../shared/http.ts";
import type { PlaybackProbeResult, ProjectsGateway } from "./gateway.ts";
import type {
  PlaybackMedia,
  PlaybackMediaEvents,
  PlaybackMediaSnapshot,
} from "./playback-media.ts";

export type JumpDirection = "back" | "forward";
export type PlaybackFailureReason =
  "auth" | "missing" | "overload" | "network" | "unsupported" | "server" | "unknown";

export interface PlaybackFailure {
  reason: PlaybackFailureReason;
  referenceId: string | null;
  checking: boolean;
}

export interface SectionPlaybackState {
  activated: boolean;
  resolving: boolean;
  url: string | null;
  generation: number;
  status: string | null;
  controlsReady: boolean;
  position: number;
  duration: number;
  rate: number;
  jumpSeconds: number;
  jumpMenu: JumpDirection | null;
  goToOpen: boolean;
  goToValue: string;
  goToError: string | null;
  failure: PlaybackFailure | null;
}

export interface PlaybackScheduler {
  schedule(delayMs: number, callback: () => void): () => void;
}

export interface SectionPlayback {
  readonly state: Readonly<Ref<SectionPlaybackState>>;
  readonly mediaEvents: PlaybackMediaEvents;
  activate(): Promise<void>;
  connect(media: PlaybackMedia): void;
  disconnect(media: PlaybackMedia): void;
  retry(): Promise<void>;
  signInAgain(): void;
  jump(direction: JumpDirection, seconds?: number): void;
  toggleJumpMenu(direction: JumpDirection): void;
  closeJumpMenu(): void;
  setRate(rate: number): void;
  openGoTo(): void;
  closeGoTo(): void;
  setGoToValue(value: string): void;
  goToTime(): void;
  dispose(): void;
}

const defaultScheduler: PlaybackScheduler = {
  schedule(delayMs, callback) {
    const timer = window.setTimeout(callback, delayMs);
    return () => window.clearTimeout(timer);
  },
};

const failureMessages: Record<PlaybackFailureReason, string> = {
  auth: "Your sign-in is no longer valid.",
  missing: "This audio is no longer available.",
  overload: "The server is handling too many audio requests.",
  network: "A network failure interrupted the audio request.",
  unsupported: "This browser could not read the audio format.",
  server: "The server could not prepare this audio.",
  unknown: "The cause could not be determined.",
};

export function playbackFailureMessage(reason: PlaybackFailureReason): string {
  return failureMessages[reason];
}

export function createSectionPlayback(options: {
  audioAssetId: string;
  gateway: ProjectsGateway;
  scheduler?: PlaybackScheduler;
  signInAgain?: () => void;
  log?: (evidence: Readonly<Record<string, unknown>>) => void;
}): SectionPlayback {
  const scheduler = options.scheduler ?? defaultScheduler;
  const state = shallowRef<SectionPlaybackState>({
    activated: false,
    resolving: false,
    url: null,
    generation: 0,
    status: null,
    controlsReady: false,
    position: 0,
    duration: 0,
    rate: 1,
    jumpSeconds: 10,
    jumpMenu: null,
    goToOpen: false,
    goToValue: "",
    goToError: null,
    failure: null,
  });
  let media: PlaybackMedia | null = null;
  let request: AbortController | null = null;
  let cancelSlow: (() => void) | null = null;
  let cancelBufferSlow: (() => void) | null = null;
  let disposed = false;
  let restorePosition = 0;

  function update(patch: Partial<SectionPlaybackState>): void {
    state.value = { ...state.value, ...patch };
  }

  function stopTimers(): void {
    cancelSlow?.();
    cancelSlow = null;
    cancelBufferSlow?.();
    cancelBufferSlow = null;
  }

  function armSlow(initial: boolean): void {
    cancelSlow?.();
    cancelSlow = scheduler.schedule(15_000, () => {
      if (disposed || state.value.failure) return;
      update({ status: initial ? "Audio is still loading…" : state.value.status });
    });
  }

  function classifyHttp(error: ApiError): PlaybackFailureReason {
    if (error.status === 401 || error.status === 403) return "auth";
    if (error.status === 404 || error.status === 410) return "missing";
    if (error.status === 429) return "overload";
    if (error.status >= 500) return "server";
    return "unknown";
  }

  function fail(reason: PlaybackFailureReason, referenceId: string | null, checking = false): void {
    stopTimers();
    update({
      resolving: false,
      status: null,
      failure: { reason, referenceId, checking },
    });
  }

  async function resolveUrl(): Promise<void> {
    request?.abort();
    const controller = new AbortController();
    request = controller;
    update({ activated: true, resolving: true, status: "Loading audio…", failure: null });
    activity("snapshot-resolve-start");
    armSlow(true);
    const timeout = scheduler.schedule(60_000, () => controller.abort());
    try {
      const url = await options.gateway.resolvePlaybackUrl(options.audioAssetId, controller.signal);
      if (disposed || request !== controller) return;
      update({
        resolving: false,
        url,
        generation: state.value.generation + 1,
        controlsReady: false,
        status: "Loading audio…",
      });
      activity("snapshot-resolve-success");
    } catch (error) {
      if (disposed || request !== controller) return;
      const reason = error instanceof ApiError ? classifyHttp(error) : "network";
      activity("snapshot-resolve-failure", {
        status: error instanceof ApiError ? error.status : "network-failure",
        referenceId: error instanceof ApiError ? (error.problem?.referenceId ?? null) : null,
      });
      fail(reason, error instanceof ApiError ? (error.problem?.referenceId ?? null) : null);
    } finally {
      timeout();
      if (request === controller) request = null;
    }
  }

  function evidence(phase: string, snapshot: PlaybackMediaSnapshot, extra = {}): void {
    options.log?.({
      phase,
      mediaErrorCode: snapshot.errorCode,
      readyState: snapshot.readyState,
      networkState: snapshot.networkState,
      ...extra,
    });
  }

  function activity(phase: string, extra: Readonly<Record<string, unknown>> = {}): void {
    options.log?.({ phase, ...extra });
  }

  async function diagnose(snapshot: PlaybackMediaSnapshot): Promise<void> {
    const url = state.value.url;
    if (!url) return;
    const controller = new AbortController();
    const timeout = scheduler.schedule(5_000, () => controller.abort());
    try {
      const outcome: PlaybackProbeResult = await options.gateway.probePlayback(
        url,
        controller.signal,
      );
      if (disposed || state.value.url !== url) return;
      evidence("media-error-probe", snapshot, { probeStatus: outcome.status });
      const fallback =
        snapshot.errorCode === 2
          ? "network"
          : [3, 4].includes(snapshot.errorCode ?? 0)
            ? "unsupported"
            : "unknown";
      fail(fallback, outcome.referenceId);
    } catch (error) {
      if (disposed || state.value.url !== url) return;
      const reason = error instanceof ApiError ? classifyHttp(error) : "network";
      const referenceId = error instanceof ApiError ? (error.problem?.referenceId ?? null) : null;
      evidence("media-error-probe", snapshot, {
        probeStatus: error instanceof ApiError ? error.status : "network-failure",
        probeReferenceId: referenceId,
      });
      fail(reason, referenceId);
    } finally {
      timeout();
    }
  }

  const mediaEvents: PlaybackMediaEvents = {
    metadata(snapshot) {
      if (!Number.isFinite(snapshot.duration) || snapshot.duration <= 0) return;
      update({ duration: snapshot.duration, position: snapshot.currentTime, controlsReady: true });
      evidence("metadata", snapshot);
    },
    playing(snapshot) {
      stopTimers();
      update({ position: snapshot.currentTime, status: null });
      evidence("playing", snapshot);
    },
    time(snapshot) {
      update({ position: snapshot.currentTime });
    },
    seeking(snapshot) {
      update({ position: snapshot.currentTime, status: "Seeking…" });
      evidence("seeking", snapshot);
    },
    seeked(snapshot) {
      update({ position: snapshot.currentTime, status: null });
      evidence("seeked", snapshot);
    },
    waiting(snapshot) {
      cancelBufferSlow?.();
      update({ position: snapshot.currentTime, status: "Buffering audio…" });
      evidence("waiting", snapshot);
      cancelBufferSlow = scheduler.schedule(15_000, () => {
        if (!state.value.failure) update({ status: "Audio is still buffering…" });
      });
    },
    error(snapshot) {
      evidence("media-error", snapshot);
      fail("unknown", null, true);
      void diagnose(snapshot);
    },
  };

  const module: SectionPlayback = {
    state: computed(() => state.value),
    mediaEvents,
    activate: resolveUrl,
    connect(nextMedia) {
      media = nextMedia;
      nextMedia.setRate(state.value.rate);
      if (restorePosition > 0) nextMedia.seek(restorePosition);
      void nextMedia.play().catch((error: unknown) => {
        if (media !== nextMedia || state.value.failure) return;
        if (error instanceof DOMException && error.name === "NotAllowedError") {
          update({ status: "Press Play to start audio." });
          return;
        }
        const snapshot = nextMedia.snapshot();
        evidence("scripted-play", snapshot);
        fail("unknown", null);
      });
    },
    disconnect(previousMedia) {
      if (media === previousMedia) media = null;
      previousMedia.dispose();
    },
    async retry() {
      restorePosition = media?.snapshot().currentTime ?? state.value.position;
      activity("retry", { retainedSnapshot: state.value.url !== null });
      update({ failure: null, status: "Restarting audio…", controlsReady: false });
      if (state.value.url) {
        update({ generation: state.value.generation + 1 });
        armSlow(true);
      } else {
        await resolveUrl();
      }
    },
    signInAgain() {
      options.signInAgain?.();
    },
    jump(direction, seconds = state.value.jumpSeconds) {
      if (!media || !state.value.controlsReady) return;
      const delta = direction === "back" ? -seconds : seconds;
      const target = Math.max(0, Math.min(state.value.duration, state.value.position + delta));
      update({ jumpSeconds: seconds, jumpMenu: null });
      media.seek(target);
    },
    toggleJumpMenu(direction) {
      update({ jumpMenu: state.value.jumpMenu === direction ? null : direction });
    },
    closeJumpMenu() {
      update({ jumpMenu: null });
    },
    setRate(rate) {
      if (![0.75, 1, 1.25, 1.5, 2].includes(rate)) return;
      update({ rate });
      media?.setRate(rate);
    },
    openGoTo() {
      update({ goToOpen: true, goToValue: "", goToError: null });
    },
    closeGoTo() {
      update({ goToOpen: false, goToValue: "", goToError: null });
    },
    setGoToValue(value) {
      update({ goToValue: value, goToError: null });
    },
    goToTime() {
      const seconds = parsePlaybackTime(state.value.goToValue);
      if (seconds === null) {
        update({ goToError: "Enter a time as M:SS or H:MM:SS." });
        return;
      }
      const target = Math.max(0, Math.min(state.value.duration, seconds));
      media?.seek(target);
      update({ goToOpen: false, goToValue: "", goToError: null });
    },
    dispose() {
      disposed = true;
      request?.abort();
      request = null;
      stopTimers();
      media?.dispose();
      media = null;
    },
  };

  return module;
}

export function parsePlaybackTime(value: string): number | null {
  const parts = value.trim().split(":");
  if (parts.length !== 2 && parts.length !== 3) return null;
  if (!parts.every((part) => /^\d+$/u.test(part))) return null;
  const numbers = parts.map(Number);
  if (parts.length === 2) {
    const [minutes, seconds] = numbers;
    return seconds! < 60 ? minutes! * 60 + seconds! : null;
  }
  const [hours, minutes, seconds] = numbers;
  return minutes! < 60 && seconds! < 60 ? hours! * 3600 + minutes! * 60 + seconds! : null;
}

export function formatPlaybackTime(seconds: number, forceHours = false): string {
  const whole = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const remainder = whole % 60;
  return hours > 0 || forceHours
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`
    : `${minutes}:${String(remainder).padStart(2, "0")}`;
}

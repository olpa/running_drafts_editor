import { describe, expect, it, vi } from "vitest";
import { ApiError } from "../shared/http.ts";
import { ManualScheduler, settle } from "../test-support/manual-scheduler.ts";
import type { ProjectsGateway } from "./gateway.ts";
import type { PlaybackMedia, PlaybackMediaSnapshot } from "./playback-media.ts";
import {
  createSectionPlayback,
  formatPlaybackTime,
  parsePlaybackTime,
} from "./section-playback.ts";

function gateway(overrides: Partial<ProjectsGateway> = {}): ProjectsGateway {
  return {
    listProjects: async () => [],
    getProjectDetail: async () => {
      throw new Error("unused");
    },
    getAudioAsset: async () => {
      throw new Error("Unexpected Audio Asset read");
    },
    confirmEnded: async () => {
      throw new Error("Unexpected confirmation");
    },
    audioDownloadUrl: () => "unused",
    resolvePlaybackUrl: async () => "https://api.example.test/snapshot/12",
    probePlayback: async () => ({ status: 206, referenceId: null }),
    ...overrides,
  };
}

class FakeMedia implements PlaybackMedia {
  value: PlaybackMediaSnapshot = {
    currentTime: 0,
    duration: 3_600,
    readyState: 1,
    networkState: 2,
    errorCode: null,
  };
  played = 0;
  seeks: number[] = [];
  rates: number[] = [];
  playFailure: unknown = null;

  snapshot(): PlaybackMediaSnapshot {
    return this.value;
  }
  async play(): Promise<void> {
    this.played++;
    if (this.playFailure) throw this.playFailure;
  }
  seek(seconds: number): void {
    this.seeks.push(seconds);
    this.value.currentTime = seconds;
  }
  setRate(rate: number): void {
    this.rates.push(rate);
  }
  dispose(): void {}
}

describe("Section playback", () => {
  it("does no playback work before explicit activation, then autostarts the resolved snapshot", async () => {
    const resolve = vi.fn(async () => "https://api.example.test/snapshot/12");
    const playback = createSectionPlayback({
      audioAssetId: "asset/id",
      gateway: gateway({ resolvePlaybackUrl: resolve }),
    });

    expect(resolve).not.toHaveBeenCalled();
    await playback.activate();
    expect(resolve).toHaveBeenCalledWith("asset/id", expect.any(AbortSignal));
    expect(playback.state.value.url).toBe("https://api.example.test/snapshot/12");

    const media = new FakeMedia();
    playback.connect(media);
    await settle();
    expect(media.played).toBe(1);
    expect(playback.state.value.status).toBe("Loading audio…");
    playback.mediaEvents.metadata(media.snapshot());
    playback.mediaEvents.playing(media.snapshot());
    expect(playback.state.value.controlsReady).toBe(true);
    expect(playback.state.value.status).toBeNull();
  });

  it("shows the still-loading and still-buffering messages without aborting media", async () => {
    const scheduler = new ManualScheduler();
    const playback = createSectionPlayback({
      audioAssetId: "asset",
      gateway: gateway(),
      scheduler,
    });
    await playback.activate();
    await scheduler.advanceBy(15_000);
    expect(playback.state.value.status).toBe("Audio is still loading…");

    const media = new FakeMedia();
    playback.connect(media);
    playback.mediaEvents.waiting(media.snapshot());
    expect(playback.state.value.status).toBe("Buffering audio…");
    await scheduler.advanceBy(15_000);
    expect(playback.state.value.status).toBe("Audio is still buffering…");
    expect(playback.state.value.failure).toBeNull();
  });

  it("uses one probe to replace a generic media error with a backend cause", async () => {
    const probe = vi.fn(async () => {
      throw new ApiError("GET", "/snapshot/12", 429, { referenceId: "safe_REF-42" });
    });
    const evidence: Record<string, unknown>[] = [];
    const playback = createSectionPlayback({
      audioAssetId: "asset",
      gateway: gateway({ probePlayback: probe }),
      log: (entry) => evidence.push({ ...entry }),
    });
    await playback.activate();
    const media = new FakeMedia();
    media.value.errorCode = 2;
    playback.connect(media);

    playback.mediaEvents.error(media.snapshot());
    expect(playback.state.value.failure).toEqual({
      reason: "unknown",
      referenceId: null,
      checking: true,
    });
    await settle();
    expect(probe).toHaveBeenCalledTimes(1);
    expect(playback.state.value.failure).toEqual({
      reason: "overload",
      referenceId: "safe_REF-42",
      checking: false,
    });
    expect(evidence).toContainEqual(
      expect.objectContaining({ phase: "media-error-probe", probeStatus: 429 }),
    );
  });

  it("falls back to MediaError only when the diagnostic probe succeeds", async () => {
    const playback = createSectionPlayback({ audioAssetId: "asset", gateway: gateway() });
    await playback.activate();
    const media = new FakeMedia();
    media.value.errorCode = 4;
    playback.connect(media);
    playback.mediaEvents.error(media.snapshot());
    await settle();
    expect(playback.state.value.failure?.reason).toBe("unsupported");
  });

  it("keeps authentication failure local and starts sign-in only on explicit activation", async () => {
    const signInAgain = vi.fn();
    const playback = createSectionPlayback({
      audioAssetId: "asset",
      gateway: gateway({
        resolvePlaybackUrl: async () => {
          throw new ApiError("HEAD", "/latest", 401, {
            code: "authentication_required",
            referenceId: "safe_AUTH-1",
          });
        },
      }),
      signInAgain,
    });

    await playback.activate();
    expect(playback.state.value.failure).toEqual({
      reason: "auth",
      referenceId: "safe_AUTH-1",
      checking: false,
    });
    expect(signInAgain).not.toHaveBeenCalled();
    playback.signInAgain();
    expect(signInAgain).toHaveBeenCalledOnce();
  });

  it("clamps navigation and shares a chosen distance in the Section", async () => {
    const playback = createSectionPlayback({ audioAssetId: "asset", gateway: gateway() });
    await playback.activate();
    const media = new FakeMedia();
    media.value.currentTime = 3_590;
    playback.connect(media);
    playback.mediaEvents.metadata(media.snapshot());
    playback.jump("forward", 900);
    expect(media.seeks.at(-1)).toBe(3_600);
    expect(playback.state.value.jumpSeconds).toBe(900);

    media.value.currentTime = 5;
    playback.mediaEvents.time(media.snapshot());
    playback.jump("back");
    expect(media.seeks.at(-1)).toBe(0);
  });

  it("validates go-to input and clamps a valid time to the duration", async () => {
    const playback = createSectionPlayback({ audioAssetId: "asset", gateway: gateway() });
    await playback.activate();
    const media = new FakeMedia();
    media.value.duration = 600;
    playback.connect(media);
    playback.mediaEvents.metadata(media.snapshot());
    playback.openGoTo();
    playback.setGoToValue("ten minutes");
    playback.goToTime();
    expect(playback.state.value.goToError).toBe("Enter a time as M:SS or H:MM:SS.");
    expect(playback.state.value.goToOpen).toBe(true);

    playback.setGoToValue("20:00");
    playback.goToTime();
    expect(media.seeks.at(-1)).toBe(600);
    expect(playback.state.value.goToOpen).toBe(false);
    expect(playback.state.value.goToValue).toBe("");
  });

  it("keeps the immutable URL and local choices when Retry replaces the player", async () => {
    const resolve = vi.fn(async () => "https://api.example.test/snapshot/12");
    const playback = createSectionPlayback({
      audioAssetId: "asset",
      gateway: gateway({ resolvePlaybackUrl: resolve }),
    });
    await playback.activate();
    const first = new FakeMedia();
    first.value.currentTime = 125;
    playback.connect(first);
    playback.mediaEvents.metadata(first.snapshot());
    playback.mediaEvents.time(first.snapshot());
    playback.setRate(1.5);
    playback.jump("forward", 300);
    first.value.errorCode = 2;
    playback.mediaEvents.error(first.snapshot());
    await settle();

    const generation = playback.state.value.generation;
    await playback.retry();
    expect(resolve).toHaveBeenCalledTimes(1);
    expect(playback.state.value.generation).toBe(generation + 1);
    expect(playback.state.value.status).toBe("Restarting audio…");
    const second = new FakeMedia();
    playback.connect(second);
    expect(second.rates).toEqual([1.5]);
    expect(second.seeks).toEqual([425]);
    expect(playback.state.value.jumpSeconds).toBe(300);
  });

  it("treats blocked autoplay as an instruction, not a playback failure", async () => {
    const playback = createSectionPlayback({ audioAssetId: "asset", gateway: gateway() });
    await playback.activate();
    const media = new FakeMedia();
    media.playFailure = new DOMException("blocked", "NotAllowedError");
    playback.connect(media);
    await settle();
    expect(playback.state.value.status).toBe("Press Play to start audio.");
    expect(playback.state.value.failure).toBeNull();
  });
});

describe("playback time", () => {
  it("parses accepted forms and rejects ambiguous values", () => {
    expect(parsePlaybackTime("7:05")).toBe(425);
    expect(parsePlaybackTime("1:07:05")).toBe(4_025);
    expect(parsePlaybackTime("7:60")).toBeNull();
    expect(parsePlaybackTime("7")).toBeNull();
  });

  it("uses an adaptive duration display", () => {
    expect(formatPlaybackTime(65.9)).toBe("1:05");
    expect(formatPlaybackTime(3_665)).toBe("1:01:05");
    expect(formatPlaybackTime(65, true)).toBe("0:01:05");
  });
});

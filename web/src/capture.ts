/** The browser-to-backend media contract for every Transport frame. */
export const TRANSPORT_FRAME_MEDIA_TYPE = "audio/webm;codecs=opus";

/** An ordered piece of one continuous recorder run, never a transcript chunk. */
export interface TransportFrame {
  seq: number;
  /** Observed timing, in milliseconds from capture start. */
  startMs: number;
  endMs: number;
  mediaType: string;
  data: Blob;
}

export interface FrameSource {
  /** Resolves when the browser accepts startup, before the first frame necessarily exists. */
  start(onFrame: (frame: TransportFrame) => void): Promise<void>;
  /** Resolves after the final data event and recorder stop event. */
  stop(): Promise<void>;
}

export function openMicrophone(): Promise<MediaStream> {
  if (!navigator.mediaDevices?.getUserMedia) {
    return Promise.reject(new Error("Microphone capture requires a supported browser and HTTPS."));
  }
  return navigator.mediaDevices.getUserMedia({
    audio: { channelCount: { ideal: 1 }, echoCancellation: true },
  });
}

export function requireTransportFrameMediaType(): string {
  if (typeof MediaRecorder !== "undefined"
    && typeof MediaRecorder.isTypeSupported === "function"
    && MediaRecorder.isTypeSupported(TRANSPORT_FRAME_MEDIA_TYPE)) {
    return TRANSPORT_FRAME_MEDIA_TYPE;
  }
  throw new Error("This browser does not support WebM/Opus microphone capture.");
}

function isWebmOpus(mediaType: string): boolean {
  const [essence = "", ...rawParameters] = mediaType.split(";");
  if (essence.trim().toLowerCase() !== "audio/webm") return false;
  const codecs = rawParameters.map((parameter) => parameter.split("=", 2))
    .find(([name]) => name?.trim().toLowerCase() === "codecs")?.[1]?.trim();
  if (!codecs) return false;
  const unquoted = codecs.replace(/^(["'])(.*)\1$/, "$2");
  return unquoted.split(",").map((codec) => codec.trim().toLowerCase()).join(",") === "opus";
}

function errorFrom(value: unknown, fallback: string): Error {
  return value instanceof Error ? value : new Error(fallback);
}

type StopMode = "explicit" | "spontaneous";
type SourceState = "ready" | "recording" | "stopping" | "terminal";

/** One single-use browser MediaRecorder run producing ordered Transport frames. */
export class MediaRecorderFrameSource implements FrameSource {
  private readonly recorder: MediaRecorder;
  private state: SourceState = "ready";
  private startedAt = 0;
  private seq = -1;
  private lastEndMs = 0;
  private onFrame: ((frame: TransportFrame) => void) | null = null;
  private stopMode: StopMode | null = null;
  private stopPromise: Promise<void> | null = null;
  private resolveStop: (() => void) | null = null;
  private rejectStop: ((error: Error) => void) | null = null;
  private finalizationTimer: ReturnType<typeof setTimeout> | null = null;
  private terminalError: Error | null = null;
  private failureReported = false;
  private settled = false;
  private listenersAttached = false;

  private readonly dataAvailable = (event: BlobEvent): void => {
    if (this.state === "terminal" || event.data.size === 0) return;
    const endMs = Math.max(this.lastEndMs + 1, Math.round(performance.now() - this.startedAt));
    this.onFrame?.({
      seq: ++this.seq,
      startMs: this.lastEndMs,
      endMs,
      mediaType: TRANSPORT_FRAME_MEDIA_TYPE,
      data: event.data,
    });
    this.lastEndMs = endMs;
    if (this.state === "recording") {
      let shouldContinue: boolean;
      try { shouldContinue = this.canContinue(); } catch (error) {
        this.interrupt(errorFrom(error, "Microphone capture continuation check failed."));
        return;
      }
      // The continuation callback may synchronously ask the caller to stop.
      if (!shouldContinue && this.state === "recording") {
        void this.beginFinalization("explicit").catch((error: unknown) => {
          this.reportSpontaneous(errorFrom(error, "Microphone capture could not be finalized."));
        });
      }
    }
  };

  private readonly recorderError = (): void => {
    const error = new Error("Microphone capture failed.");
    if (this.state === "stopping" && this.stopMode === "explicit") {
      this.finish(error);
      return;
    }
    this.interrupt(error);
  };

  private readonly recorderStopped = (): void => {
    if (this.state === "recording") {
      const error = new Error("Microphone capture ended.");
      void this.beginFinalization("spontaneous");
      this.reportSpontaneous(error);
      return;
    }
    this.finish();
  };

  constructor(
    stream: MediaStream,
    private readonly intervalMs: number,
    /** Called after each non-empty frame; false stops and flushes the final partial frame. */
    private readonly canContinue: () => boolean,
    private readonly onError: (error: Error) => void,
    private readonly finalizationTimeoutMs = 3_000,
    private readonly onSettled: () => void = () => {},
  ) {
    this.recorder = new MediaRecorder(stream, { mimeType: requireTransportFrameMediaType() });
    if (!isWebmOpus(this.recorder.mimeType)) {
      throw new Error("The browser did not create a WebM/Opus microphone recorder.");
    }
  }

  async start(onFrame: (frame: TransportFrame) => void): Promise<void> {
    if (this.state !== "ready") {
      throw this.terminalError ?? new Error("Capture has already started or ended.");
    }
    this.onFrame = onFrame;
    this.attachListeners();
    this.startedAt = performance.now();
    this.state = "recording";
    try {
      this.recorder.start(this.intervalMs);
    } catch (error) {
      const startupError = errorFrom(error, "Microphone capture could not start.");
      this.terminalError = startupError;
      this.finish();
      throw startupError;
    }
  }

  stop(): Promise<void> {
    if (this.state === "terminal") {
      return this.stopPromise ?? (this.terminalError
        ? Promise.reject(this.terminalError) : Promise.resolve());
    }
    if (this.state === "stopping") return this.stopPromise!;
    if (this.state === "ready") {
      this.stopPromise = Promise.resolve();
      this.finish();
      return this.stopPromise;
    }
    return this.beginFinalization("explicit");
  }

  /** Stop immediately and release listeners. A partial final frame is best effort only. */
  dispose(): void {
    if (this.state === "terminal") return;
    const wasStopping = this.state === "stopping";
    try {
      if (this.recorder.state !== "inactive") this.recorder.stop();
    } catch {
      // Teardown is best effort and never throws.
    }
    if (wasStopping) this.finish(new Error("Microphone capture was disposed before finalization completed."));
    else this.finish();
  }

  /** Report an owned-track failure. Before start, the later start operation rejects it. */
  interrupt(error: Error): void {
    if (this.state === "terminal") return;
    if (this.state === "ready") {
      this.terminalError = error;
      this.finish();
      return;
    }
    if (this.state === "stopping") {
      if (this.stopMode === "explicit") this.finish(error);
      return;
    }
    void this.beginFinalization("spontaneous");
    this.reportSpontaneous(error);
  }

  private beginFinalization(mode: StopMode): Promise<void> {
    if (this.stopPromise) return this.stopPromise;
    this.state = "stopping";
    this.stopMode = mode;
    this.stopPromise = new Promise<void>((resolve, reject) => {
      this.resolveStop = resolve;
      this.rejectStop = reject;
    });
    try {
      if (this.recorder.state === "inactive") {
        if (mode === "spontaneous") this.finish();
        else this.finish(new Error("Microphone capture ended before finalization."));
        return this.stopPromise;
      }
      this.recorder.stop();
      if ((this.state as SourceState) === "terminal") return this.stopPromise;
      this.finalizationTimer = setTimeout(() => {
        const error = new Error("Microphone capture did not finish within the finalization deadline.");
        if (mode === "spontaneous") this.finish();
        else this.finish(error);
      }, this.finalizationTimeoutMs);
    } catch (error) {
      const stopError = errorFrom(error, "Microphone capture could not be finalized.");
      if (mode === "spontaneous") this.finish();
      else this.finish(stopError);
    }
    return this.stopPromise;
  }

  private reportSpontaneous(error: Error): void {
    if (this.failureReported) return;
    this.failureReported = true;
    this.terminalError = error;
    try { this.onError(error); } catch (callbackError) {
      console.error("Capture failure callback threw.", callbackError);
    }
  }

  private finish(error?: Error): void {
    if (this.state === "terminal") return;
    if (this.finalizationTimer !== null) clearTimeout(this.finalizationTimer);
    this.finalizationTimer = null;
    this.state = "terminal";
    this.detachOperationalListeners();
    this.attachLateEventWarnings();
    if (!this.settled) {
      this.settled = true;
      try { this.onSettled(); } catch (cleanupError) {
        console.error("Capture resource cleanup failed.", cleanupError);
      }
    }
    if (!this.stopPromise) {
      this.stopPromise = error ? Promise.reject(error) : Promise.resolve();
      if (error) void this.stopPromise.catch(() => {});
      return;
    }
    if (error) this.rejectStop?.(error);
    else this.resolveStop?.();
    this.resolveStop = null;
    this.rejectStop = null;
  }

  private attachListeners(): void {
    if (this.listenersAttached) return;
    this.listenersAttached = true;
    this.recorder.addEventListener("dataavailable", this.dataAvailable);
    this.recorder.addEventListener("error", this.recorderError);
    this.recorder.addEventListener("stop", this.recorderStopped);
  }

  private detachOperationalListeners(): void {
    if (!this.listenersAttached) return;
    this.listenersAttached = false;
    this.recorder.removeEventListener("dataavailable", this.dataAvailable);
    this.recorder.removeEventListener("error", this.recorderError);
    this.recorder.removeEventListener("stop", this.recorderStopped);
  }

  private attachLateEventWarnings(): void {
    for (const type of ["dataavailable", "error", "stop"] as const) {
      this.recorder.addEventListener(type, () => {
        console.warn(`Ignored late MediaRecorder ${type} event after capture ended.`);
      });
    }
  }
}

export interface CaptureOptions {
  intervalMs: number;
  canContinue: () => boolean;
  onError: (error: Error) => void;
  /** Maximum wait for MediaRecorder's final data and stop events. Defaults to three seconds. */
  finalizationTimeoutMs?: number;
}

export interface MicrophoneCapture extends FrameSource {
  /** Immediately release recorder and microphone, including after failed startup. */
  dispose(): void;
}

/** Browser adapter owning microphone acquisition, recorder finalization, and resource release. */
export async function openMicrophoneCapture(options: CaptureOptions): Promise<MicrophoneCapture> {
  requireTransportFrameMediaType();
  const stream = await openMicrophone();
  const tracks = stream.getTracks();
  let source: MediaRecorderFrameSource | null = null;
  let released = false;
  const trackEnded = (): void => source?.interrupt(new Error("Microphone capture ended."));
  const release = (): void => {
    if (released) return;
    released = true;
    for (const track of tracks) {
      try { track.removeEventListener("ended", trackEnded); } catch (error) {
        console.error("Could not detach a microphone track listener.", error);
      }
      try { track.stop(); } catch (error) {
        console.error("Could not release a microphone track.", error);
      }
    }
  };
  try {
    const audioTracks = stream.getAudioTracks();
    if (audioTracks.length !== 1 || audioTracks[0]!.readyState === "ended") {
      throw new Error("The microphone did not provide one usable audio track.");
    }
    source = new MediaRecorderFrameSource(stream, options.intervalMs, options.canContinue,
      options.onError, options.finalizationTimeoutMs, release);
    for (const track of tracks) track.addEventListener("ended", trackEnded);
    return {
      start: (onFrame) => source!.start(onFrame),
      stop: () => source!.stop(),
      dispose: () => source!.dispose(),
    };
  } catch (error) {
    release();
    throw error;
  }
}

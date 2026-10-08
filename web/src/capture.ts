/** An ordered piece of one continuous recording, never a transcript chunk. */
export interface TransportFrame {
  seq: number;
  /** Observed timing, in milliseconds from capture start. */
  startMs: number;
  endMs: number;
  mediaType: string;
  data: Blob;
}

export interface FrameSource {
  start(onFrame: (frame: TransportFrame) => void): void;
  /** Resolves after the final data event and recorder stop event. */
  stop(): Promise<void>;
}

export function openMicrophone(): Promise<MediaStream> {
  if (!navigator.mediaDevices?.getUserMedia) {
    return Promise.reject(new Error("Microphone capture requires a supported browser and HTTPS."));
  }
  return navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true } });
}

export function recordingMediaType(): string {
  if (typeof MediaRecorder !== "undefined") {
    for (const type of ["audio/webm;codecs=opus", "audio/webm"]) {
      if (MediaRecorder.isTypeSupported(type)) return type;
    }
  }
  throw new Error("This browser does not support WebM audio recording.");
}

/** One recorder; requestData splits delivery without restarting capture. */
export class RecorderFrameSource implements FrameSource {
  private readonly recorder: MediaRecorder;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private startedAt = 0;
  private seq = 0;
  private lastEndMs = 0;
  private stopping = false;
  private stopped: Promise<void> | null = null;

  constructor(
    stream: MediaStream,
    private readonly intervalMs: number,
    /** False stops at this boundary instead of opening another delivery frame. */
    private readonly canContinue: () => boolean,
    private readonly onError: (error: Error) => void,
  ) {
    this.recorder = new MediaRecorder(stream, { mimeType: recordingMediaType() });
  }

  start(onFrame: (frame: TransportFrame) => void): void {
    if (this.stopped) throw new Error("Capture has already started.");
    this.stopped = new Promise((resolve) => {
      this.recorder.addEventListener("stop", () => {
        const unexpected = !this.stopping;
        this.stopping = true;
        this.clearTimer();
        resolve();
        if (unexpected) this.onError(new Error("Microphone recording ended."));
      }, { once: true });
    });
    this.recorder.addEventListener("dataavailable", (event) => {
      if (event.data.size > 0) {
        const endMs = Math.round(performance.now() - this.startedAt);
        onFrame({
          seq: ++this.seq,
          startMs: this.lastEndMs,
          endMs,
          mediaType: this.recorder.mimeType,
          data: event.data,
        });
        this.lastEndMs = endMs;
      }
      if (!this.stopping) this.scheduleBoundary();
    });
    this.recorder.addEventListener("error", () => {
      this.onError(new Error("Microphone recording failed."));
    });
    this.startedAt = performance.now();
    this.recorder.start();
    this.scheduleBoundary();
  }

  stop(): Promise<void> {
    this.stopping = true;
    this.clearTimer();
    if (this.recorder.state !== "inactive") this.recorder.stop();
    return this.stopped ?? Promise.resolve();
  }

  private scheduleBoundary(): void {
    this.clearTimer();
    this.timer = setTimeout(() => {
      this.timer = null;
      if (!this.canContinue()) {
        void this.stop();
      } else {
        try {
          this.recorder.requestData();
        } catch (error) {
          this.onError(error instanceof Error ? error : new Error(String(error)));
        }
      }
    }, this.intervalMs);
  }

  private clearTimer(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}

export interface CaptureOptions {
  intervalMs: number;
  canContinue: () => boolean;
  onError: (error: Error) => void;
}

export interface RecordingCapture extends FrameSource {
  /** Release recorder and microphone, including after failed startup. */
  dispose(): void;
}

/** Browser adapter: microphone ownership stays with the capture implementation. */
export async function openRecordingCapture(options: CaptureOptions): Promise<RecordingCapture> {
  recordingMediaType();
  const stream = await openMicrophone();
  const tracks = stream.getTracks();
  const ended = (): void => options.onError(new Error("Microphone recording ended."));
  const release = (): void => {
    for (const track of tracks) {
      track.removeEventListener("ended", ended);
      track.stop();
    }
  };
  try {
    const source = new RecorderFrameSource(stream, options.intervalMs, options.canContinue, options.onError);
    for (const track of stream.getAudioTracks()) track.addEventListener("ended", ended);
    return {
      start: (onFrame) => source.start(onFrame),
      stop: async () => {
        try { await source.stop(); } finally { release(); }
      },
      dispose: () => {
        void source.stop();
        release();
      },
    };
  } catch (error) {
    release();
    throw error;
  }
}

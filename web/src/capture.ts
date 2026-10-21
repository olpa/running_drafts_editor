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
  if (typeof navigator.mediaDevices.getSupportedConstraints !== "function"
    || navigator.mediaDevices.getSupportedConstraints().channelCount !== true) {
    return Promise.reject(new Error("This browser cannot request mono microphone capture."));
  }
  return navigator.mediaDevices.getUserMedia({
    audio: { channelCount: { exact: 1 }, echoCancellation: true },
  });
}

export function recordingMediaType(): string {
  const mediaType = "audio/webm;codecs=opus";
  if (typeof MediaRecorder !== "undefined" && MediaRecorder.isTypeSupported(mediaType)) {
    return mediaType;
  }
  throw new Error("This browser does not support WebM/Opus microphone capture.");
}

/** One continuous recorder using the browser's configured delivery timeslice. */
export class RecorderFrameSource implements FrameSource {
  private readonly recorder: MediaRecorder;
  private startedAt = 0;
  private seq = -1;
  private lastEndMs = 0;
  private stopping = false;
  private stopped: Promise<void> | null = null;

  constructor(
    stream: MediaStream,
    private readonly intervalMs: number,
    /** Called after each non-empty frame; false stops and flushes the final partial frame. */
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
        resolve();
        if (unexpected) this.onError(new Error("Microphone capture ended."));
      }, { once: true });
    });
    this.recorder.addEventListener("dataavailable", (event) => {
      if (event.data.size > 0) {
        const endMs = Math.max(this.lastEndMs + 1, Math.round(performance.now() - this.startedAt));
        onFrame({
          seq: ++this.seq,
          startMs: this.lastEndMs,
          endMs,
          mediaType: this.recorder.mimeType,
          data: event.data,
        });
        this.lastEndMs = endMs;
        if (!this.stopping && !this.canContinue()) void this.stop();
      }
    });
    this.recorder.addEventListener("error", () => {
      this.onError(new Error("Microphone capture failed."));
    });
    this.startedAt = performance.now();
    this.recorder.start(this.intervalMs);
  }

  stop(): Promise<void> {
    this.stopping = true;
    if (this.recorder.state !== "inactive") this.recorder.stop();
    return this.stopped ?? Promise.resolve();
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
  const ended = (): void => options.onError(new Error("Microphone capture ended."));
  const release = (): void => {
    for (const track of tracks) {
      track.removeEventListener("ended", ended);
      track.stop();
    }
  };
  try {
    const audioTracks = stream.getAudioTracks();
    if (audioTracks.length !== 1 || audioTracks[0]!.getSettings().channelCount !== 1) {
      throw new Error("The microphone did not provide one-channel audio.");
    }
    const source = new RecorderFrameSource(stream, options.intervalMs, options.canContinue, options.onError);
    for (const track of audioTracks) track.addEventListener("ended", ended);
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

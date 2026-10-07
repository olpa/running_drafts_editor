/**
 * An independently processable piece of browser-captured audio, identified by
 * its recording and an increasing sequence number. It is never a chunk.
 */
export interface TransportFrame {
  seq: number;
  /** Observed timing, in milliseconds from the start of the recording. */
  startMs: number;
  endMs: number;
  mediaType: string;
  data: Blob;
}

export interface FrameSource {
  start(onFrame: (frame: TransportFrame) => void): void;
  /** Stops producing frames and emits the final partial frame. */
  stop(): void;
}

export function openMicrophone(): Promise<MediaStream> {
  return navigator.mediaDevices.getUserMedia({ audio: true });
}

export const MOCK_FRAME_MEDIA_TYPE = "application/x-mock-frame";

/**
 * Produces placeholder frames on a timer. #77 replaces it with MediaRecorder
 * frames recorded from the open microphone stream.
 */
export class MockFrameSource implements FrameSource {
  private timer: number | null = null;
  private startedAt = 0;
  private seq = 0;
  private lastEndMs = 0;
  private onFrame: ((frame: TransportFrame) => void) | null = null;

  constructor(private readonly intervalMs: number) {}

  start(onFrame: (frame: TransportFrame) => void): void {
    this.onFrame = onFrame;
    this.startedAt = performance.now();
    this.timer = window.setInterval(() => this.emit(), this.intervalMs);
  }

  stop(): void {
    if (this.timer !== null) window.clearInterval(this.timer);
    this.timer = null;
    this.emit();
    this.onFrame = null;
  }

  private emit(): void {
    const endMs = Math.round(performance.now() - this.startedAt);
    this.seq += 1;
    const frame: TransportFrame = {
      seq: this.seq,
      startMs: this.lastEndMs,
      endMs,
      mediaType: MOCK_FRAME_MEDIA_TYPE,
      data: new Blob([`mock frame ${this.seq}`]),
    };
    this.lastEndMs = endMs;
    this.onFrame?.(frame);
  }
}

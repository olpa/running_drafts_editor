import type { TransportFrame } from "./capture.js";
import type { FrameAck } from "./api.js";

export interface DeliveryOptions {
  retryDelaysMs: readonly number[];
  requestTimeoutMs: number;
  send: (frame: TransportFrame, signal: AbortSignal) => Promise<FrameAck>;
  log: (message: string) => void;
  changed: () => void;
  acknowledged: (frame: TransportFrame) => void;
  delayed: () => void;
}

/** Retains ordered bytes until success, independent of capture and processing. */
export class FrameDelivery {
  private readonly frames: TransportFrame[] = [];
  private lastSequence = 0;
  private current: TransportFrame | null = null;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private automatic = true;
  private disposed = false;
  private readonly requests = new Map<AbortController, ReturnType<typeof setTimeout>>();
  delayed = false;

  constructor(private readonly options: DeliveryOptions) {}

  get pendingCount(): number { return this.frames.length; }
  get pendingMs(): number {
    return this.frames.reduce((sum, frame) => sum + frame.endMs - frame.startMs, 0);
  }

  enqueue(frame: TransportFrame): void {
    if (frame.seq !== this.lastSequence + 1) throw new Error("Transport frame sequence is not consecutive.");
    this.lastSequence = frame.seq;
    this.frames.push(frame);
    this.options.changed();
    this.pump();
  }

  /** Stops new attempts, preserving bytes and allowing existing requests to settle. */
  freeze(): void {
    this.automatic = false;
    this.clearRetry();
    this.current = null;
    this.options.changed();
  }

  retry(): void {
    this.automatic = true;
    this.clearRetry();
    this.current = null;
    this.pump();
  }

  dispose(): void {
    this.disposed = true;
    this.freeze();
    for (const [controller, timer] of this.requests) {
      clearTimeout(timer);
      controller.abort();
    }
    this.requests.clear();
  }

  private pump(): void {
    if (this.disposed || !this.automatic || this.current || !this.frames[0]) return;
    this.current = this.frames[0];
    this.attempt(this.current, 0);
  }

  private attempt(frame: TransportFrame, attempt: number): void {
    const controller = new AbortController();
    const startedAt = performance.now();
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      this.requests.delete(controller);
      controller.abort();
      if (!this.disposed && this.frames.includes(frame)) {
        this.options.log(`f${frame.seq} timeout`);
      }
    }, this.options.requestTimeoutMs);
    this.requests.set(controller, timer);
    this.options.log(`f${frame.seq} sent ${frame.data.size}B a${attempt + 1}`);
    void this.options.send(frame, controller.signal).then((ack) => {
      if (this.disposed || timedOut || !this.frames.includes(frame)) return;
      if (ack.seq !== frame.seq || ack.acknowledged !== true) {
        throw new Error(`Unexpected acknowledgement for frame ${frame.seq}.`);
      }
      if (this.frames[0] !== frame) throw new Error("Acknowledgement changed transport order.");
      this.frames.shift();
      this.current = null;
      this.clearRetry();
      this.options.log(`f${frame.seq} ack ${Math.round(performance.now() - startedAt)}ms`);
      if (!this.pendingCount) this.delayed = false;
      this.options.acknowledged(frame);
      this.options.changed();
      this.pump();
    }).catch((error: unknown) => {
      if (!this.disposed && !timedOut && this.frames.includes(frame)) {
        this.options.log(`f${frame.seq} ERR ${error instanceof Error ? error.message : String(error)}`);
      }
    }).finally(() => {
      clearTimeout(timer);
      this.requests.delete(controller);
    });

    const delay = this.options.retryDelaysMs[attempt];
    if (delay !== undefined && this.automatic) {
      this.retryTimer = setTimeout(() => {
        this.retryTimer = null;
        if (!this.automatic || this.frames[0] !== frame) return;
        if (!this.delayed) {
          this.delayed = true;
          this.options.delayed();
          this.options.changed();
        }
        this.options.log(`f${frame.seq} retry ${attempt + 1}`);
        this.attempt(frame, attempt + 1);
      }, delay);
    }
  }

  private clearRetry(): void {
    if (this.retryTimer !== null) clearTimeout(this.retryTimer);
    this.retryTimer = null;
  }
}

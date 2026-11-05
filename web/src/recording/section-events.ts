import type { StopReason } from "./section-delivery.ts";

export type SectionEvent =
  | { type: "captureStarted" }
  | { type: "transportFrameSecured"; frameId: number; captureActive: boolean; finalFrame: boolean }
  | { type: "transportFrameOverdue"; frameId: number; elapsedSeconds: number }
  | { type: "audioAssetSealed" }
  | { type: "captureFinalizationFailed"; reason: StopReason; error: unknown }
  | { type: "audioAssetSealingFailed"; error: unknown }
  | { type: "unrequestedCaptureStopped" };

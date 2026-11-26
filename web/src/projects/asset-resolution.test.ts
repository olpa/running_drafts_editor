import { describe, expect, it, vi } from "vitest";
import { ApiError } from "../shared/http.ts";
import { createAssetResolution } from "./asset-resolution.ts";
import type { AudioAssetRead } from "./gateway.ts";

const now = "2026-11-25T12:01:00Z";
const initial: AudioAssetRead = {
  serverTime: now,
  audioAsset: {
    id: "asset",
    createdAt: "2026-11-25T11:00:00Z",
    updatedAt: "2026-11-25T12:00:00Z",
    mediaType: "audio/webm;codecs=opus",
    frameCount: 2,
    observedDurationMs: 2000,
    storedBytes: 20,
    isSealed: false,
    sealReason: null,
  },
};

const fresh = { ...initial, audioAsset: { ...initial.audioAsset, frameCount: 3 } };
const sealed: AudioAssetRead = {
  ...initial,
  audioAsset: { ...initial.audioAsset, isSealed: true, sealReason: "user_resolved" },
};
const failure = (status: number, code?: string) =>
  new ApiError("POST", "/audio-assets/asset/confirm-ended", status, code ? { code } : null);

function setup(snapshot = initial) {
  const gateway = {
    getAudioAsset: vi.fn<(...args: [string]) => Promise<AudioAssetRead>>(async () => fresh),
    confirmEnded: vi.fn<(...args: [string, number]) => Promise<AudioAssetRead["audioAsset"]>>(
      async () => sealed.audioAsset,
    ),
  };
  return { gateway, card: createAssetResolution(gateway, snapshot) };
}

describe("Unfinished Audio Asset", () => {
  it("uses the later usable timestamp and the response clock with the cutoff at one minute", () => {
    expect(setup().card.recent.value).toBe(false);
    const recent = setup({
      ...initial,
      audioAsset: { ...initial.audioAsset, createdAt: now, updatedAt: "2026-11-25T12:02:00Z" },
    });
    expect(recent.card.recent.value).toBe(true);
    expect(setup({ ...initial, serverTime: "invalid" }).card.recent.value).toBe(false);
  });

  it("rechecks only this asset and fails closed if the read fails", async () => {
    const { gateway, card } = setup();
    const other = setup();
    gateway.getAudioAsset.mockRejectedValueOnce(new Error("offline"));
    await card.refresh();
    await card.confirm();
    expect(gateway.confirmEnded).not.toHaveBeenCalled();
    expect(card.retryRead.value).toBe(true);
    await card.refresh();
    expect(gateway.getAudioAsset.mock.calls).toEqual([["asset"], ["asset"]]);
    expect(card.asset.value.frameCount).toBe(3);
    expect(other.card.asset.value.frameCount).toBe(2);
  });

  it("never submits an invalid frame count", async () => {
    const { gateway, card } = setup({
      ...initial,
      audioAsset: { ...initial.audioAsset, frameCount: -1 },
    });
    await card.confirm();
    expect(gateway.confirmEnded).not.toHaveBeenCalled();
  });

  it("requires a fresh confirmation after the asset changes", async () => {
    const { gateway, card } = setup();
    gateway.confirmEnded.mockRejectedValueOnce(failure(409, "audio_asset_changed"));
    await card.confirm();
    expect(gateway.confirmEnded.mock.calls).toEqual([["asset", 2]]);
    expect(card.message.value).toContain("More audio");
    gateway.getAudioAsset.mockResolvedValueOnce(sealed);
    await card.confirm();
    expect(gateway.confirmEnded.mock.calls).toEqual([
      ["asset", 2],
      ["asset", 3],
    ]);
    expect(card.asset.value).toEqual(sealed.audioAsset);
  });

  it.each(["completed", "interrupted", "user_resolved"] as const)(
    "recognizes a terminal conflict and reloads an asset sealed as %s",
    async (sealReason) => {
      const { gateway, card } = setup();
      gateway.confirmEnded.mockRejectedValueOnce(failure(409, "audio_asset_sealed"));
      gateway.getAudioAsset.mockResolvedValueOnce({
        ...sealed,
        audioAsset: { ...sealed.audioAsset, sealReason },
      });
      await card.confirm();
      expect(card.open.value).toBe(false);
      expect(card.asset.value.sealReason).toBe(sealReason);
    },
  );

  it("retains the known seal when the follow-up read fails", async () => {
    const { gateway, card } = setup();
    gateway.getAudioAsset.mockRejectedValueOnce(new Error("offline"));
    await card.confirm();
    expect(card.open.value).toBe(false);
    expect(card.announcement.value).toBe("Recording sealed.");
    expect(card.retryRead.value).toBe(true);
  });

  it("reconciles a timeout but leaves a server error inline with the same action", async () => {
    const { gateway, card } = setup();
    gateway.confirmEnded.mockRejectedValueOnce(failure(503));
    await card.confirm();
    expect(card.message.value).toBe("The server returned an error. Try confirming again.");
    expect(gateway.getAudioAsset).not.toHaveBeenCalled();
    gateway.confirmEnded.mockRejectedValueOnce(new DOMException("Timed out", "TimeoutError"));
    gateway.getAudioAsset.mockResolvedValueOnce(sealed);
    await card.confirm();
    expect(gateway.getAudioAsset).toHaveBeenCalledOnce();
    expect(card.open.value).toBe(false);
  });

  it.each([401, 404])("keeps HTTP %s local to the selected asset", async (status) => {
    const { gateway, card } = setup();
    gateway.getAudioAsset.mockRejectedValueOnce(failure(status));
    await card.refresh();
    expect(card.auth.value).toBe(status === 401);
    expect(card.unavailable.value).toBe(status === 404);
    expect(card.retryRead.value).toBe(false);
  });

  it("ignores a response after the card is disposed", async () => {
    const { card } = setup();
    const read = card.refresh();
    card.dispose();
    await read;
    expect(card.asset.value.frameCount).toBe(2);
  });
});

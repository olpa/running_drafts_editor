import { describe, expect, it, vi } from "vitest";
import { createMicrophoneAccess } from "./microphone-access.ts";

describe("MicrophoneAccess", () => {
  it("reserves immediately, releases normally, and ignores stale releases", async () => {
    const access = createMicrophoneAccess();
    const cleanup = vi.fn();
    const old = access.acquire(cleanup)!;
    expect(access.acquire(vi.fn())).toBeNull();
    const pending = access.requestTakeover();
    expect(access.takeoverRequested.value).toBe(true);
    access.confirmTakeover();
    expect(await pending).toBe(true);
    expect(cleanup).toHaveBeenCalledOnce();
    expect(old.active).toBe(false);
    const next = access.acquire(vi.fn())!;
    old.release();
    expect(next.active).toBe(true);
    expect(access.acquire(vi.fn())).toBeNull();
    next.release();
    expect(access.acquire(vi.fn())).not.toBeNull();
    access.dispose();
  });
  it("cancels without touching the current holder, including disposal of the modal", async () => {
    const access = createMicrophoneAccess();
    const cleanup = vi.fn();
    const holder = access.acquire(cleanup)!;
    const pending = access.requestTakeover();
    access.cancelTakeover();
    expect(await pending).toBe(false);
    expect(holder.active).toBe(true);
    expect(cleanup).not.toHaveBeenCalled();
    const second = access.requestTakeover();
    access.dispose();
    expect(await second).toBe(false);
    expect(cleanup).toHaveBeenCalledOnce();
  });
  it("does not grant new access if forced cleanup throws", async () => {
    const access = createMicrophoneAccess();
    vi.spyOn(console, "error").mockImplementation(() => {});
    const old = access.acquire(() => {
      throw new Error("cleanup failed");
    })!;
    const pending = access.requestTakeover();
    access.confirmTakeover();
    expect(await pending).toBe(false);
    expect(old.active).toBe(true);
    expect(access.acquire(() => {})).toBeNull();
    old.release();
    vi.restoreAllMocks();
  });
});

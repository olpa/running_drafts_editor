import { computed, ref, shallowRef } from "vue";
import { ApiError } from "../shared/http.ts";
import type { AudioAssetRead, ProjectsGateway } from "./gateway.ts";

function timestamp(value: unknown): number {
  const dateTime = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/u;
  return typeof value === "string" && dateTime.test(value) ? Date.parse(value) : NaN;
}

export function createAssetResolution(
  gateway: Pick<ProjectsGateway, "getAudioAsset" | "confirmEnded">,
  initial: AudioAssetRead,
) {
  const snapshot = shallowRef(initial);
  const asset = computed(() => snapshot.value.audioAsset);
  const id = asset.value.id;
  const busy = ref(false);
  const unavailable = ref(false);
  const auth = ref(false);
  const retryRead = ref(false);
  const sealKnown = ref(false);
  const message = ref("");
  const announcement = ref("");
  let disposed = false;

  const open = computed(() => !sealKnown.value && asset.value.isSealed === false);
  const validCount = computed(
    () => Number.isSafeInteger(asset.value.frameCount) && asset.value.frameCount >= 0,
  );
  const lastActivity = computed(() => {
    const server = timestamp(snapshot.value.serverTime);
    const usable = [timestamp(asset.value.createdAt), timestamp(asset.value.updatedAt)].filter(
      (time) => time <= server,
    );
    return usable.length ? Math.max(...usable) : null;
  });
  const recent = computed(
    () =>
      lastActivity.value !== null &&
      timestamp(snapshot.value.serverTime) - lastActivity.value < 60_000,
  );

  function handleUnavailable(error: unknown): boolean {
    if (!(error instanceof ApiError)) return false;
    if (error.status === 401) {
      auth.value = true;
      message.value = "Your sign-in is no longer valid.";
    } else if (error.status === 404) {
      unavailable.value = true;
      message.value = "This audio is no longer available";
    } else return false;
    return true;
  }

  async function read(): Promise<void> {
    try {
      const fresh = await gateway.getAudioAsset(id);
      if (disposed) return;
      if (fresh.audioAsset.id !== id || (sealKnown.value && fresh.audioAsset.isSealed === false)) {
        throw new Error("Unexpected Audio Asset state");
      }
      if (fresh.audioAsset.isSealed === true) {
        if (open.value) announcement.value = "Recording sealed.";
        message.value = "";
      }
      snapshot.value = fresh;
      retryRead.value = false;
    } catch (error) {
      if (disposed) return;
      retryRead.value = !handleUnavailable(error);
      if (retryRead.value) message.value = "Could not refresh this audio.";
    }
  }

  async function run(action: () => Promise<void>): Promise<void> {
    if (disposed || busy.value || unavailable.value || auth.value) return;
    busy.value = true;
    message.value = "";
    try {
      await action();
    } finally {
      if (!disposed) busy.value = false;
    }
  }

  function acknowledgeSeal(): void {
    sealKnown.value = true;
    announcement.value = "Recording sealed.";
  }

  async function confirm(): Promise<void> {
    if (!open.value || retryRead.value || recent.value || !validCount.value) return;
    await run(async () => {
      try {
        await gateway.confirmEnded(id, asset.value.frameCount);
        if (disposed) return;
        acknowledgeSeal();
      } catch (error) {
        if (disposed || handleUnavailable(error)) return;
        if (error instanceof ApiError) {
          const code = error.problem?.code;
          if (error.status === 409 && code === "audio_asset_sealed") acknowledgeSeal();
          else if (error.status === 409 && code === "audio_asset_changed") {
            message.value = "More audio was secured before confirmation.";
          } else {
            message.value =
              error.status >= 500
                ? "The server returned an error. Try confirming again."
                : "Could not confirm that recording ended. Try confirming again.";
            return;
          }
        } else if (
          !(error instanceof DOMException || error instanceof Error) ||
          error.name !== "TimeoutError"
        ) {
          message.value = "Could not confirm that recording ended. Try confirming again.";
          return;
        }
        // A timeout can leave the command outcome unknown.
      }
      await read();
    });
  }

  return {
    asset,
    open,
    validCount,
    recent,
    lastActivity,
    busy,
    unavailable,
    auth,
    retryRead,
    message,
    announcement,
    refresh: () => run(read),
    confirm,
    dispose() {
      disposed = true;
    },
  };
}

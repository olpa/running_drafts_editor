export interface PlaybackMediaSnapshot {
  currentTime: number;
  duration: number;
  readyState: number;
  networkState: number;
  errorCode: number | null;
}

export interface PlaybackMediaEvents {
  metadata(snapshot: PlaybackMediaSnapshot): void;
  playing(snapshot: PlaybackMediaSnapshot): void;
  time(snapshot: PlaybackMediaSnapshot): void;
  seeking(snapshot: PlaybackMediaSnapshot): void;
  seeked(snapshot: PlaybackMediaSnapshot): void;
  waiting(snapshot: PlaybackMediaSnapshot): void;
  error(snapshot: PlaybackMediaSnapshot): void;
}

/** Browser media operations used by one Section playback module. */
export interface PlaybackMedia {
  snapshot(): PlaybackMediaSnapshot;
  play(): Promise<void>;
  seek(seconds: number): void;
  setRate(rate: number): void;
  dispose(): void;
}

export function connectBrowserPlaybackMedia(
  element: HTMLAudioElement,
  events: PlaybackMediaEvents,
): PlaybackMedia {
  const snapshot = (): PlaybackMediaSnapshot => ({
    currentTime: element.currentTime,
    duration: element.duration,
    readyState: element.readyState,
    networkState: element.networkState,
    errorCode: element.error?.code ?? null,
  });
  const listeners: Array<[keyof HTMLMediaElementEventMap, EventListener]> = [];

  function listen(name: keyof HTMLMediaElementEventMap, callback: () => void): void {
    const listener = () => callback();
    listeners.push([name, listener]);
    element.addEventListener(name, listener);
  }

  listen("loadedmetadata", () => events.metadata(snapshot()));
  listen("playing", () => events.playing(snapshot()));
  listen("timeupdate", () => events.time(snapshot()));
  listen("seeking", () => events.seeking(snapshot()));
  listen("seeked", () => events.seeked(snapshot()));
  listen("waiting", () => events.waiting(snapshot()));
  listen("error", () => events.error(snapshot()));

  return {
    snapshot,
    play: () => element.play(),
    seek(seconds) {
      element.currentTime = seconds;
    },
    setRate(rate) {
      element.playbackRate = rate;
    },
    dispose() {
      for (const [name, listener] of listeners) element.removeEventListener(name, listener);
    },
  };
}

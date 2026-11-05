import type { Cue, Cues } from "../cues.ts";
import {
  normalizeSound,
  synthesizeSound,
  type SoundDefinition,
  type SoundRole,
} from "../sound-definitions.ts";
import { candidate, candidates, fixedStart, type Candidate, type Role } from "./candidates.ts";

export interface SoundSettings {
  choices: Record<Role, string>;
  levels: Record<Role, number>;
  master: number;
  pitch: number;
  duration: number;
  problemGap: number;
  unavailable: boolean;
}

const bounded = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

export function synthesize(
  value: Candidate,
  sampleRate: number,
  pitch = 0,
  stretch = 1,
): Float32Array {
  if (!value.notes?.length) throw new Error(`No synthesis definition for ${value.id}`);
  return synthesizeSound(value as SoundDefinition, sampleRate, pitch, stretch);
}

export const normalize = normalizeSound;

/** Story-only sound adapter. Repeating playback uses real WebAudio time. */
export function createLabSound(
  settings: SoundSettings,
  log: (message: string) => void,
  failure: (message: string) => void,
) {
  let context: AudioContext | null = null;
  let disposed = false;
  let ready = false;
  let enabling: Promise<void> | null = null;
  const downloaded = new Map<string, AudioBuffer>();
  const sources = new Set<AudioBufferSourceNode>();
  let activeProblem: { unavailable: () => void } | null = null;

  const fail = (message: string) => {
    failure(message);
    log(`Audio unavailable: ${message}`);
  };

  function stateChanged(): void {
    if (!disposed && ready && context?.state !== "running") {
      fail("Browser audio playback was suspended or closed.");
      activeProblem?.unavailable();
    }
  }

  function requireAudio(): AudioContext {
    if (!ready || !context || context.state !== "running" || settings.unavailable) {
      throw new Error(
        settings.unavailable
          ? "Playback failure is simulated."
          : "Enable laboratory sound with the explicit button.",
      );
    }
    return context;
  }

  function buffer(value: Candidate | SoundDefinition, audio: AudioContext): AudioBuffer {
    if ("asset" in value && value.asset) {
      const found = downloaded.get(value.id);
      if (!found) throw new Error(`Local audio was not loaded: ${value.name}`);
      return found;
    }
    const samples = synthesizeSound(
      value as SoundDefinition,
      audio.sampleRate,
      settings.pitch,
      settings.duration,
    );
    const result = audio.createBuffer(1, samples.length, audio.sampleRate);
    result.copyToChannel(Float32Array.from(samples), 0);
    return result;
  }

  function play(role: SoundRole, loop: boolean): () => void {
    const value =
      role === "capture-started" ? fixedStart : candidate(settings.choices[role as Role]);
    log(`sound: ${value.id} (${value.name})${loop ? "; repeating" : ""}`);
    const audio = requireAudio();
    let audioBuffer = buffer(value, audio);
    if (loop && settings.problemGap > 0) {
      const padded = audio.createBuffer(
        audioBuffer.numberOfChannels,
        audioBuffer.length + Math.round(audio.sampleRate * bounded(settings.problemGap, 0, 3)),
        audio.sampleRate,
      );
      for (let channel = 0; channel < audioBuffer.numberOfChannels; channel++) {
        padded.copyToChannel(Float32Array.from(audioBuffer.getChannelData(channel)), channel);
      }
      audioBuffer = padded;
    }
    const source = audio.createBufferSource();
    const gain = audio.createGain();
    source.buffer = audioBuffer;
    source.loop = loop;
    gain.gain.value =
      bounded(settings.master, 0, 1) *
      (role === "capture-started" ? 1 : bounded(settings.levels[role as Role], 0, 1.5));
    source.connect(gain).connect(audio.destination);
    sources.add(source);
    let cleaned = false;
    const clean = () => {
      if (cleaned) return;
      cleaned = true;
      sources.delete(source);
      source.disconnect();
      gain.disconnect();
    };
    source.onended = clean;
    try {
      source.start();
    } catch (error) {
      clean();
      throw error;
    }
    let stopped = false;
    return () => {
      if (stopped) return;
      stopped = true;
      source.stop();
      clean();
    };
  }

  const cues: Cues = {
    prepare() {},
    play(cue: Cue) {
      try {
        play(cue, false);
      } catch (error) {
        fail(String(error));
      }
    },
    loopProblem(unavailable) {
      const activation = { unavailable };
      activeProblem = activation;
      try {
        const stop = play("problem", true);
        return () => {
          if (activeProblem === activation) activeProblem = null;
          stop();
        };
      } catch (error) {
        fail(String(error));
        unavailable();
        return () => {
          if (activeProblem === activation) activeProblem = null;
        };
      }
    },
    dispose() {
      disposed = true;
      ready = false;
      activeProblem = null;
      for (const source of sources) source.stop();
      sources.clear();
      if (context) {
        context.removeEventListener("statechange", stateChanged);
        void context.close().catch(() => {});
      }
      context = null;
    },
  };

  async function enable(): Promise<void> {
    if (enabling) return enabling;
    enabling = (async () => {
      try {
        if (disposed) return;
        if (!context) {
          context = new AudioContext();
          context.addEventListener("statechange", stateChanged);
        }
        const audio = context;
        await audio.resume();
        for (const value of candidates.filter((item) => item.asset)) {
          if (downloaded.has(value.id)) continue;
          const response = await fetch(value.asset!);
          if (!response.ok) throw new Error(`Could not load bundled candidate ${value.id}`);
          const decoded = await audio.decodeAudioData(await response.arrayBuffer());
          if (disposed) return;
          const samples = new Float32Array(decoded.length);
          for (let channel = 0; channel < decoded.numberOfChannels; channel++) {
            const input = decoded.getChannelData(channel);
            for (let index = 0; index < samples.length; index++) {
              samples[index] = samples[index]! + input[index]! / decoded.numberOfChannels;
            }
          }
          const normalized = audio.createBuffer(1, samples.length, decoded.sampleRate);
          normalized.copyToChannel(Float32Array.from(normalizeSound(samples)), 0);
          downloaded.set(value.id, normalized);
        }
        ready = true;
        failure("");
        log("Laboratory audio enabled; all downloaded candidates are bundled locally.");
      } catch (error) {
        fail(String(error));
      } finally {
        enabling = null;
      }
    })();
    return enabling;
  }

  return { cues, enable };
}

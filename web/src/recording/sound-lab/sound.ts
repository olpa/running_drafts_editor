import type { Cue, Cues } from "../cues.ts";
import { readonly, shallowRef } from "vue";
import {
  candidate,
  candidates,
  fixedStart,
  type Candidate,
  type Note,
  type Role,
} from "./candidates.ts";

export interface SoundSettings {
  choices: Record<Role, string>;
  levels: Record<Role, number>;
  master: number;
  pitch: number;
  duration: number;
  alarmGap: number;
  unavailable: boolean;
  repeatProblem: boolean;
}
const bounded = (n: number, low: number, high: number) => Math.min(high, Math.max(low, n));

/** Deterministic voices, with a short attack and no random browser noise source. */
export function synthesize(
  value: Candidate,
  sampleRate: number,
  pitch = 0,
  stretch = 1,
): Float32Array {
  if (!value.notes?.length) throw new Error(`No synthesis definition for ${value.id}`);
  const ratio = value.baseline ? 1 : 2 ** (bounded(pitch, -12, 12) / 12);
  const scale = value.baseline ? 1 : bounded(stretch, 0.5, 2);
  const duration = (value.duration ?? Math.max(...value.notes.map((n) => n.at + n.length))) * scale;
  const samples = new Float32Array(Math.ceil(duration * sampleRate));
  for (const n of value.notes) {
    const first = Math.round(n.at * scale * sampleRate);
    const length = n.length * scale;
    for (let i = 0; i < length * sampleRate && first + i < samples.length; i++) {
      const t = i / sampleRate;
      const cycles = ratio * (n.from * t + ((n.to - n.from) * t * t) / (2 * length));
      const envelope = value.baseline
        ? 1 - t / length
        : Math.min(1, t / 0.004) * (1 - t / length) ** 2;
      samples[first + i] = (samples[first + i] ?? 0) + voice(n, cycles, t) * envelope * 0.08;
    }
  }
  return value.baseline ? samples : normalize(samples);
}
function voice(note: Note, cycles: number, t: number): number {
  const phase = 2 * Math.PI * cycles;
  switch (note.voice) {
    case "sine":
      return Math.sin(phase);
    case "triangle":
      return (2 / Math.PI) * Math.asin(Math.sin(phase));
    case "bell":
      return Math.sin(phase) + 0.35 * Math.sin(phase * 2.76) * Math.exp(-14 * t);
    case "wood":
      return Math.sin(phase) * Math.exp(-30 * t) + 0.3 * Math.sin(phase * 1.73) * Math.exp(-55 * t);
    case "noise":
      return (Math.sin(phase * 1.79) + Math.sin(phase * 2.93) + Math.sin(phase * 4.71)) / 3;
  }
}
/** Shared RMS target with a peak cap; silence cannot be amplified into noise. */
export function normalize(samples: Float32Array): Float32Array {
  let energy = 0,
    peak = 0;
  for (const sample of samples) {
    energy += sample * sample;
    peak = Math.max(peak, Math.abs(sample));
  }
  const rms = Math.sqrt(energy / samples.length);
  const gain = rms > 0.000001 ? Math.min(0.028 / rms, 0.08 / peak) : 0;
  return samples.map((sample) => sample * gain);
}

/** Story-only audio adapter. Alarm sources use WebAudio looping, never simulation timers. */
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
  let alarm: { unavailable: () => void } | null = null;
  const problemActive = shallowRef(false);
  let stopRepeating: (() => void) | null = null;
  let repeatingKey = "";
  const fail = (message: string) => {
    failure(message);
    log(`Audio unavailable: ${message}`);
  };
  function stateChanged(): void {
    if (!disposed && ready && context?.state !== "running") {
      fail("Browser audio playback was suspended or closed.");
      alarm?.unavailable();
    }
  }
  function requireAudio(): AudioContext {
    if (!ready || !context || context.state !== "running" || settings.unavailable)
      throw new Error(
        settings.unavailable
          ? "Playback failure is simulated."
          : "Enable laboratory sound with the explicit button.",
      );
    return context;
  }
  function buffer(value: Candidate, ctx: AudioContext): AudioBuffer {
    if (value.asset) {
      const found = downloaded.get(value.id);
      if (!found) throw new Error(`Local audio was not loaded: ${value.name}`);
      return found;
    }
    const samples = synthesize(value, ctx.sampleRate, settings.pitch, settings.duration);
    const result = ctx.createBuffer(1, samples.length, ctx.sampleRate);
    result.copyToChannel(Float32Array.from(samples), 0);
    return result;
  }
  function play(role: Cue, loop: boolean): () => void {
    const value = role === "start" ? fixedStart : candidate(settings.choices[role]);
    log(`sound: ${value.id} (${value.name})${loop ? "; repeating" : ""}`);
    const ctx = requireAudio();
    let audio = buffer(value, ctx);
    if (loop && settings.alarmGap > 0) {
      const padded = ctx.createBuffer(
        audio.numberOfChannels,
        audio.length + Math.round(ctx.sampleRate * bounded(settings.alarmGap, 0, 3)),
        ctx.sampleRate,
      );
      for (let ch = 0; ch < audio.numberOfChannels; ch++)
        padded.copyToChannel(Float32Array.from(audio.getChannelData(ch)), ch);
      audio = padded;
    }
    const source = ctx.createBufferSource();
    const gain = ctx.createGain();
    source.buffer = audio;
    source.loop = loop;
    gain.gain.value =
      bounded(settings.master, 0, 1) *
      (role === "start" ? 1 : bounded(settings.levels[role], 0, 1.5));
    source.connect(gain).connect(ctx.destination);
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
      if (!stopped) {
        stopped = true;
        source.stop();
        clean();
      }
    };
  }
  function refreshAlarm(force = false): void {
    if (disposed) return;
    if (!settings.repeatProblem) problemActive.value = false;
    const role = alarm ? "interrupted" : problemActive.value ? "delayed" : null;
    const key = role
      ? JSON.stringify([
          settings.choices[role],
          settings.levels[role],
          settings.master,
          settings.pitch,
          settings.duration,
          settings.alarmGap,
          settings.unavailable,
        ])
      : "";
    if (!force && key === repeatingKey) {
      if (role && !stopRepeating) alarm?.unavailable();
      return;
    }
    stopRepeating?.();
    stopRepeating = null;
    repeatingKey = key;
    if (!role) return;
    try {
      stopRepeating = play(role, true);
    } catch (error) {
      fail(String(error));
      alarm?.unavailable();
    }
  }
  function acknowledgeProblem(): void {
    if (!problemActive.value) return;
    problemActive.value = false;
    log("Experimental Problem repetition cleared.");
    refreshAlarm();
  }
  const cues: Cues = {
    prepare() {},
    play(cue: Cue) {
      try {
        if (cue === "health") acknowledgeProblem();
        if (cue === "delayed" && settings.repeatProblem) {
          if (!problemActive.value) log("Experimental Problem repetition activated.");
          problemActive.value = true;
          refreshAlarm();
          return;
        }
        play(cue, false);
      } catch (error) {
        fail(String(error));
      }
    },
    loopInterrupted(unavailable) {
      const activation = { unavailable };
      alarm = activation;
      refreshAlarm();
      return () => {
        if (alarm !== activation) return;
        alarm = null;
        refreshAlarm();
      };
    },
    dispose() {
      disposed = true;
      ready = false;
      alarm = null;
      problemActive.value = false;
      stopRepeating?.();
      stopRepeating = null;
      repeatingKey = "";
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
        const ctx = context;
        await ctx.resume();
        for (const value of candidates.filter((item) => item.asset)) {
          if (downloaded.has(value.id)) continue;
          const response = await fetch(value.asset!);
          if (!response.ok) throw new Error(`Could not load bundled candidate ${value.id}`);
          const decoded = await ctx.decodeAudioData(await response.arrayBuffer());
          if (disposed) return;
          // Mix to mono so headphones and ordinary speakers hear the same candidate.
          const samples = new Float32Array(decoded.length);
          for (let ch = 0; ch < decoded.numberOfChannels; ch++) {
            const channel = decoded.getChannelData(ch);
            for (let i = 0; i < samples.length; i++)
              samples[i] = samples[i]! + channel[i]! / decoded.numberOfChannels;
          }
          const normalized = ctx.createBuffer(1, samples.length, decoded.sampleRate);
          normalized.copyToChannel(Float32Array.from(normalize(samples)), 0);
          downloaded.set(value.id, normalized);
        }
        ready = true;
        failure("");
        log("Laboratory audio enabled; all downloaded candidates are bundled locally.");
        refreshAlarm(true);
      } catch (error) {
        fail(String(error));
      } finally {
        enabling = null;
      }
    })();
    return enabling;
  }
  return { cues, enable, refreshAlarm, acknowledgeProblem, problemActive: readonly(problemActive) };
}

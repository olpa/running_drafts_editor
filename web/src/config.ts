export interface SeedConfig {
  /** Length of one transport frame. */
  frameIntervalMs: number;
  /** Automatic-stop limit; enforced from #77. */
  maxRecordingMs: number;
  /** Interval between recording-status polls. */
  pollIntervalMs: number;
  logColumns: number;
  logRows: number;
  /** Blank characters cleared ahead of the diagnostic log's write position. */
  logGapChars: number;
}

declare global {
  interface Window {
    /** Overrides injected before page load, for example by browser tests. */
    __SEED_CONFIG__?: Partial<SeedConfig>;
  }
}

export const DEFAULT_CONFIG: Readonly<SeedConfig> = {
  frameIntervalMs: 25_000,
  maxRecordingMs: 10 * 60_000,
  pollIntervalMs: 1_000,
  logColumns: 100,
  logRows: 10,
  logGapChars: 10,
};

export function loadConfig(overrides: Partial<SeedConfig> = window.__SEED_CONFIG__ ?? {}): SeedConfig {
  const config: SeedConfig = { ...DEFAULT_CONFIG };
  for (const [key, value] of Object.entries(overrides)) {
    if (!(key in DEFAULT_CONFIG)) throw new Error(`Unknown seed config key: ${key}`);
    if (!Number.isInteger(value) || (value as number) <= 0) {
      throw new Error(`Seed config ${key} must be a positive integer, got ${String(value)}`);
    }
    config[key as keyof SeedConfig] = value as number;
  }
  if (config.logGapChars >= config.logColumns * config.logRows) {
    throw new Error("Seed config logGapChars must be smaller than the log size");
  }
  return config;
}

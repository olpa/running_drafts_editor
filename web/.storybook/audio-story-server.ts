import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { Plugin } from "vite";

const LATEST_PATH = "/__running-drafts/audio/latest";
const SNAPSHOT_PATH = "/__running-drafts/audio/snapshots/one-hour.webm";
const SCENARIO_PATH = "/__running-drafts/audio/scenario";
const ONE_HOUR_SECONDS = 3_600;
const CLUSTER_SECONDS = 10;
const CONDITION_FLAGS = [
  "auth",
  "missing",
  "overload",
  "server",
  "network",
  "invalid",
  "slow",
  "buffer",
] as const;

interface EbmlElement {
  id: number;
  payloadStart: number;
  payloadEnd: number;
}

function fixturePath(): string {
  const relative = "e2e/fixtures/check-english.webm";
  const candidates = [resolve(process.cwd(), relative), resolve(process.cwd(), "web", relative)];
  const path = candidates.find((candidate) => existsSync(candidate));
  if (!path) throw new Error(`Audio story fixture is missing: ${relative}`);
  return path;
}

function vint(data: Buffer, offset: number, preserveMarker: boolean): [number, number] {
  const first = data[offset];
  if (first === undefined || first === 0) throw new Error("Invalid story WebM integer.");
  let width = 1;
  let marker = 0x80;
  while ((first & marker) === 0) {
    width++;
    marker >>= 1;
  }
  if (offset + width > data.length) throw new Error("Truncated story WebM integer.");
  let value = preserveMarker ? first : first & (marker - 1);
  for (let index = 1; index < width; index++) value = value * 256 + data[offset + index]!;
  return [value, width];
}

function elements(data: Buffer, start: number, end: number): EbmlElement[] {
  const result: EbmlElement[] = [];
  let offset = start;
  while (offset < end) {
    const [elementId, idWidth] = vint(data, offset, true);
    const [payloadSize, sizeWidth] = vint(data, offset + idWidth, false);
    const payloadStart = offset + idWidth + sizeWidth;
    const payloadEnd = payloadStart + payloadSize;
    if (payloadEnd > end) throw new Error("Truncated story WebM element.");
    result.push({ id: elementId, payloadStart, payloadEnd });
    offset = payloadEnd;
  }
  return result;
}

/** Read the 20 ms Opus packets from the checked-in, user-provided voice fixture. */
export function voiceFixturePackets(): Buffer[] {
  const source = readFileSync(fixturePath());
  const segment = elements(source, 0, source.length).find((element) => element.id === 0x18538067);
  if (!segment) throw new Error("Story WebM has no Segment.");
  const packets: Buffer[] = [];
  for (const cluster of elements(source, segment.payloadStart, segment.payloadEnd)) {
    if (cluster.id !== 0x1f43b675) continue;
    for (const block of elements(source, cluster.payloadStart, cluster.payloadEnd)) {
      if (block.id !== 0xa3) continue;
      const [, trackWidth] = vint(source, block.payloadStart, false);
      const flags = source[block.payloadStart + trackWidth + 2];
      if (flags === undefined || (flags & 0x06) !== 0) {
        throw new Error("Laced story WebM blocks are unsupported.");
      }
      packets.push(source.subarray(block.payloadStart + trackWidth + 3, block.payloadEnd));
    }
  }
  if (packets.length === 0) throw new Error("Story WebM has no Opus packets.");
  return packets;
}

const OPUS_PACKETS = voiceFixturePackets();

const IDS = {
  ebml: 0x1a45dfa3,
  segment: 0x18538067,
  info: 0x1549a966,
  tracks: 0x1654ae6b,
  trackEntry: 0xae,
  audio: 0xe1,
  cluster: 0x1f43b675,
  timecode: 0xe7,
  simpleBlock: 0xa3,
};

function id(value: number): Buffer {
  let hex = value.toString(16);
  if (hex.length % 2) hex = `0${hex}`;
  return Buffer.from(hex, "hex");
}

function size(value: number, width = 8): Buffer {
  const result = Buffer.alloc(width);
  let encoded = BigInt(value) | (1n << BigInt(7 * width));
  for (let index = width - 1; index >= 0; index--) {
    result[index] = Number(encoded & 0xffn);
    encoded >>= 8n;
  }
  return result;
}

function element(elementId: number, value: Buffer, sizeWidth = 8): Buffer {
  return Buffer.concat([id(elementId), size(value.length, sizeWidth), value]);
}

function uint(elementId: number, value: number, width = 8): Buffer {
  const data = Buffer.alloc(width);
  let remaining = BigInt(value);
  for (let index = width - 1; index >= 0; index--) {
    data[index] = Number(remaining & 0xffn);
    remaining >>= 8n;
  }
  return element(elementId, data);
}

function float64(elementId: number, value: number): Buffer {
  const data = Buffer.alloc(8);
  data.writeDoubleBE(value);
  return element(elementId, data);
}

function simpleBlock(relativeTicks: number, packet: Buffer): Buffer {
  const prefix = Buffer.alloc(4);
  prefix[0] = 0x81;
  prefix.writeInt16BE(relativeTicks, 1);
  prefix[3] = 0x80;
  return element(IDS.simpleBlock, Buffer.concat([prefix, packet]), 4);
}

function canonicalHeader(durationTicks: number): [Buffer, Buffer] {
  const ebml = element(
    IDS.ebml,
    Buffer.concat([
      uint(0x4286, 1, 1),
      uint(0x42f7, 1, 1),
      uint(0x42f2, 4, 1),
      uint(0x42f3, 8, 1),
      element(0x4282, Buffer.from("webm")),
      uint(0x4287, 4, 1),
      uint(0x4285, 2, 1),
    ]),
  );
  const info = element(
    IDS.info,
    Buffer.concat([
      uint(0x2ad7b1, 500_000, 4),
      float64(0x4489, durationTicks),
      element(0x4d80, Buffer.from("Running Drafts story")),
      element(0x5741, Buffer.from("Running Drafts story")),
    ]),
  );
  const audio = element(IDS.audio, Buffer.concat([float64(0xb5, 48_000), uint(0x9f, 1, 1)]));
  const codecPrivate = Buffer.from("T3B1c0hlYWQBATgBgLsAAAAAAA==", "base64");
  const entry = element(
    IDS.trackEntry,
    Buffer.concat([
      uint(0xd7, 1, 1),
      uint(0x73c5, 1),
      uint(0x83, 2, 1),
      uint(0x9c, 0, 1),
      element(0x86, Buffer.from("A_OPUS")),
      element(0x63a2, codecPrivate),
      uint(0x56aa, 6_500_000),
      uint(0x56bb, 80_000_000),
      audio,
    ]),
  );
  return [ebml, Buffer.concat([info, element(IDS.tracks, entry)])];
}

function cues(clusterTicks: readonly number[], positions: readonly number[]): Buffer {
  const points = clusterTicks.map((ticks, index) => {
    const position = element(
      0xb7,
      Buffer.concat([uint(0xf7, 1, 1), uint(0xf1, positions[index]!, 8)]),
    );
    return element(0xbb, Buffer.concat([uint(0xb3, ticks, 8), position]));
  });
  return element(0x1c53bb6b, Buffer.concat(points));
}

export function buildOneHourStoryWebm(): Buffer {
  const clusterCount = ONE_HOUR_SECONDS / CLUSTER_SECONDS;
  const packetsPerCluster = CLUSTER_SECONDS * 50;
  const clusters: Buffer[] = [];
  const ticks: number[] = [];
  for (let clusterIndex = 0; clusterIndex < clusterCount; clusterIndex++) {
    const clusterTicks = clusterIndex * CLUSTER_SECONDS * 2_000;
    ticks.push(clusterTicks);
    const blocks: Buffer[] = [uint(IDS.timecode, clusterTicks, 8)];
    for (let packetIndex = 0; packetIndex < packetsPerCluster; packetIndex++) {
      blocks.push(simpleBlock(packetIndex * 40, OPUS_PACKETS[packetIndex % OPUS_PACKETS.length]!));
    }
    clusters.push(element(IDS.cluster, Buffer.concat(blocks)));
  }

  const [ebml, fixedPrefix] = canonicalHeader(ONE_HOUR_SECONDS * 2_000);
  const placeholder = cues(
    ticks,
    ticks.map(() => 0),
  );
  const positions: number[] = [];
  let position = fixedPrefix.length + placeholder.length;
  for (const cluster of clusters) {
    positions.push(position);
    position += cluster.length;
  }
  const segment = Buffer.concat([fixedPrefix, cues(ticks, positions), ...clusters]);
  return Buffer.concat([ebml, element(IDS.segment, segment)]);
}

function problem(status: number, code: string, referenceId: string): string {
  return JSON.stringify({ status, code, referenceId, title: "Story playback failure" });
}

/** Same-origin HTTP mock used only by the audio-player Storybook story. */
export function audioStoryServer(): Plugin {
  let resource: Buffer | null = null;
  const scenarios = new Map<string, ReadonlySet<string>>();
  return {
    name: "running-drafts-audio-story-server",
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const url = new URL(request.url ?? "/", "http://storybook.local");
        const token = url.searchParams.get("story") ?? "";
        if (url.pathname === SCENARIO_PATH && request.method === "POST") {
          scenarios.set(
            token,
            new Set(CONDITION_FLAGS.filter((flag) => url.searchParams.has(flag))),
          );
          response.statusCode = 204;
          response.end();
          return;
        }
        if (url.pathname !== LATEST_PATH && url.pathname !== SNAPSHOT_PATH) return next();
        const conditions =
          scenarios.get(token) ??
          new Set(CONDITION_FLAGS.filter((flag) => url.searchParams.has(flag)));
        const enabled = (flag: (typeof CONDITION_FLAGS)[number]) => conditions.has(flag);

        const finish = () => {
          if (enabled("network")) {
            request.socket.destroy();
            return;
          }
          const errors = [
            ["auth", 401, "authentication_required", "story_AUTH-401"],
            ["missing", 404, "playback_snapshot_not_available", "story_MISSING-404"],
            ["overload", 429, "playback_concurrency_limit_exceeded", "story_OVERLOAD-429"],
            ["server", 500, "playback_representation_corrupt", "story_SERVER-500"],
          ] as const;
          const selected = errors.find(([flag]) => enabled(flag));
          if (selected) {
            response.statusCode = selected[1];
            response.setHeader("Content-Type", "application/problem+json");
            response.end(problem(selected[1], selected[2], selected[3]));
            return;
          }

          if (url.pathname === LATEST_PATH) {
            response.statusCode = 307;
            response.setHeader("Location", `${SNAPSHOT_PATH}${url.search}`);
            response.end();
            return;
          }

          resource ??= buildOneHourStoryWebm();
          const body = enabled("invalid") ? Buffer.from("not valid WebM") : resource;
          response.setHeader("Accept-Ranges", "bytes");
          response.setHeader("Cache-Control", "private, max-age=31536000, immutable");
          response.setHeader("Content-Type", "audio/webm; codecs=opus");
          response.setHeader("ETag", '"storybook-one-hour-v1"');
          response.setHeader("Content-Length", body.length);
          if (request.method === "HEAD") {
            response.statusCode = 200;
            response.end();
            return;
          }

          const match = /^bytes=(\d+)-(\d*)$/u.exec(request.headers.range ?? "");
          const requestedStart = match ? Number(match[1]) : 0;
          const requestedEnd = match?.[2] ? Number(match[2]) : body.length - 1;
          const fiveSecondBytes = Math.max(1, Math.ceil(body.length / (ONE_HOUR_SECONDS / 5)));
          const start = Math.min(requestedStart, body.length - 1);
          const end = Math.min(requestedEnd, start + fiveSecondBytes - 1, body.length - 1);
          const chunk = body.subarray(start, end + 1);
          response.statusCode = 206;
          response.setHeader("Content-Range", `bytes ${start}-${end}/${body.length}`);
          response.setHeader("Content-Length", chunk.length);
          response.end(chunk);
        };

        const delay =
          (url.pathname === LATEST_PATH && enabled("slow")) ||
          (url.pathname === SNAPSHOT_PATH && enabled("buffer"))
            ? 2_000
            : 0;
        if (delay) {
          setTimeout(finish, delay);
        } else finish();
      });
    },
  };
}

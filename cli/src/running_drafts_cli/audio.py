from __future__ import annotations

import math
import subprocess
import tempfile
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path


class AudioPreparationError(Exception):
    """The source audio could not become a transport stream."""


@dataclass(frozen=True)
class PreparedFrame:
    sequence: int
    start_ms: int
    end_ms: int
    chunk_path: Path
    header_path: Path | None = None

    def read(self) -> AudioFrame:
        prefix = self.header_path.read_bytes() if self.header_path is not None else b""
        return AudioFrame(
            self.sequence, self.start_ms, self.end_ms, prefix + self.chunk_path.read_bytes()
        )


@dataclass(frozen=True)
class AudioFrame:
    sequence: int
    start_ms: int
    end_ms: int
    payload: bytes


@dataclass(frozen=True)
class PreparedAudio:
    duration_ms: int
    frames: tuple[PreparedFrame, ...]


def _duration_ms(progress: str) -> int:
    values = [
        int(line.removeprefix("out_time_us="))
        for line in progress.splitlines()
        if line.startswith("out_time_us=")
    ]
    if not values or values[-1] <= 0:
        raise AudioPreparationError("ffmpeg did not report a positive output duration")
    return math.ceil(values[-1] / 1000)


@contextmanager
def prepare_audio(source: Path, *, ffmpeg: str, frame_duration_ms: int) -> Iterator[PreparedAudio]:
    if not source.is_file():
        raise AudioPreparationError(f"audio file does not exist: {source}")
    with tempfile.TemporaryDirectory(prefix="rde-upload-") as temporary_directory:
        temporary = Path(temporary_directory)
        header = temporary / "header.webm"
        pattern = temporary / "frame-%09d.chk"
        command = [
            ffmpeg,
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostats",
            "-progress",
            "pipe:1",
            "-y",
            "-i",
            str(source),
            "-map",
            "0:a:0",
            "-vn",
            "-c:a",
            "libopus",
            "-b:a",
            "32k",
            "-f",
            "webm_chunk",
            "-audio_chunk_duration",
            str(frame_duration_ms),
            "-header",
            str(header),
            str(pattern),
        ]
        try:
            result = subprocess.run(command, text=True, capture_output=True, check=False)
        except OSError as error:
            raise AudioPreparationError(f"could not run ffmpeg: {error}") from None
        if result.returncode != 0:
            detail = result.stderr.strip() or "ffmpeg exited without diagnostics"
            raise AudioPreparationError(f"ffmpeg conversion failed: {detail}")

        duration_ms = _duration_ms(result.stdout)
        chunk_paths = sorted(temporary.glob("frame-*.chk"))
        if not header.is_file() or header.stat().st_size == 0 or not chunk_paths:
            raise AudioPreparationError("ffmpeg produced an incomplete WebM/Opus stream")
        expected_count = math.ceil(duration_ms / frame_duration_ms)
        if len(chunk_paths) != expected_count:
            raise AudioPreparationError(
                "ffmpeg produced a frame count inconsistent with its output duration"
            )

        frames = []
        for sequence, chunk_path in enumerate(chunk_paths):
            start_ms = sequence * frame_duration_ms
            end_ms = min((sequence + 1) * frame_duration_ms, duration_ms)
            if chunk_path.stat().st_size == 0 or end_ms <= start_ms:
                raise AudioPreparationError("ffmpeg produced an invalid transport frame")
            frames.append(
                PreparedFrame(
                    sequence,
                    start_ms,
                    end_ms,
                    chunk_path,
                    header if sequence == 0 else None,
                )
            )
        yield PreparedAudio(duration_ms, tuple(frames))

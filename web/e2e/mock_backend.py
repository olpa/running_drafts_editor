"""Mock seed backend, installed by intercepting the page's /api requests.

It implements the assumed contract in ``src/api.ts`` and follows the script in
``fixtures/mock-backend.json``: a chunk becomes visible after a given transport
frame is acknowledged or after the recording is finished. A chunk whose
``endMs`` is null ends where the last transport frame ends. After finish, the
recording reports ``finishing`` until it has been polled
``completeAfterFinishPolls`` times.

#78 deletes this module and runs the same test against the real backend.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import Page, Route


@dataclass
class ReceivedFrame:
    seq: int
    start_ms: int
    end_ms: int
    media_type: str
    size: int


@dataclass
class MockRecording:
    recording_id: str
    language: str
    frames: list[ReceivedFrame] = field(default_factory=list)
    finished: bool = False
    polls_since_finish: int = 0


class MockBackend:
    def __init__(self, script: dict) -> None:
        self.script = script
        self.recordings: list[MockRecording] = []
        self.violations: list[str] = []

    @classmethod
    def load(cls, path: Path) -> MockBackend:
        return cls(json.loads(path.read_text(encoding="utf-8")))

    def install(self, page: Page) -> None:
        page.route("**/api/**", self._handle)

    def _handle(self, route: Route) -> None:
        request = route.request
        path = urlparse(request.url).path
        method = request.method
        if method == "POST" and path == "/api/recordings":
            return self._create(route)
        if match := re.fullmatch(r"/api/recordings/([^/]+)/frames/(\d+)", path):
            if method == "PUT":
                return self._frame(route, self._recording(match[1]), int(match[2]))
        if match := re.fullmatch(r"/api/recordings/([^/]+)/finish", path):
            if method == "POST":
                return self._finish(route, self._recording(match[1]))
        if match := re.fullmatch(r"/api/recordings/([^/]+)", path):
            if method == "GET":
                return self._status(route, self._recording(match[1]))
        self.violations.append(f"unexpected request: {method} {path}")
        route.fulfill(status=404, json={"error": "not found"})

    def _recording(self, recording_id: str) -> MockRecording:
        for recording in self.recordings:
            if recording.recording_id == recording_id:
                return recording
        raise AssertionError(f"unknown recording ID: {recording_id}")

    def _create(self, route: Route) -> None:
        body = route.request.post_data_json
        recording = MockRecording(f"mock-recording-{len(self.recordings) + 1}", body["language"])
        self.recordings.append(recording)
        route.fulfill(json={"recordingId": recording.recording_id})

    def _frame(self, route: Route, recording: MockRecording, seq: int) -> None:
        if recording.finished:
            self.violations.append(f"frame {seq} received after finish")
        expected_seq = len(recording.frames) + 1
        if seq > expected_seq:
            self.violations.append(f"frame {seq} received before frame {expected_seq}")
        if seq == expected_seq:
            headers = route.request.headers
            recording.frames.append(
                ReceivedFrame(
                    seq=seq,
                    start_ms=int(headers["x-frame-start-ms"]),
                    end_ms=int(headers["x-frame-end-ms"]),
                    media_type=headers["content-type"],
                    size=len(route.request.post_data_buffer or b""),
                )
            )
        route.fulfill(json={"seq": seq, "acknowledged": True})

    def _finish(self, route: Route, recording: MockRecording) -> None:
        recording.finished = True
        route.fulfill(status=204)

    def _status(self, route: Route, recording: MockRecording) -> None:
        if recording.finished:
            recording.polls_since_finish += 1
            done = recording.polls_since_finish >= self.script["completeAfterFinishPolls"]
            status = "complete" if done else "finishing"
        else:
            status = "recording"
        route.fulfill(json={"status": status, "chunks": self._visible_chunks(recording)})

    def _visible_chunks(self, recording: MockRecording) -> list[dict]:
        last_end_ms = recording.frames[-1].end_ms if recording.frames else 0
        chunks = []
        for chunk in self.script["chunks"]:
            after_frame = chunk.get("visibleAfterFrame")
            visible = (after_frame is not None and len(recording.frames) >= after_frame) or (
                chunk.get("visibleAfterFinish", False) and recording.finished
            )
            if visible:
                chunks.append(
                    {
                        "id": chunk["id"],
                        "startMs": chunk["startMs"],
                        "endMs": chunk["endMs"] if chunk["endMs"] is not None else last_end_ms,
                        "text": chunk["text"],
                    }
                )
        return chunks

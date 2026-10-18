from __future__ import annotations

import base64
import hashlib
import json
import os
import socket
import subprocess
import sys
from collections.abc import Iterator
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Event, Thread
from uuid import UUID

PROJECT_ID = "3c754425-b730-4b5d-a65a-4b5d7019c5f7"
SECTION_ID = "e1407eb9-d4cb-44cc-ad50-dc6fd4aa63ac"
AUDIO_ASSET_ID = "45b701d9-45db-474c-864a-8fa5d2546e9b"
FAKE_FFMPEG = Path(__file__).parent / "fixtures" / "fake_ffmpeg"


class ProjectApiHandler(BaseHTTPRequestHandler):
    requests: list[dict[str, object]] = []

    def do_POST(self) -> None:  # noqa: N802
        length = int(self.headers["Content-Length"])
        body = json.loads(self.rfile.read(length))
        self.__class__.requests.append(
            {"method": "POST", "path": self.path, "headers": self.headers, "body": body}
        )
        response = {
            "id": PROJECT_ID,
            "title": body["title"],
            "createdAt": "2026-10-14T10:00:00Z",
            "updatedAt": "2026-10-14T10:00:00Z",
        }
        payload = json.dumps(response).encode()
        self.send_response(201)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Location", f"/api/v1/projects/{PROJECT_ID}")
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self) -> None:  # noqa: N802
        self.__class__.requests.append(
            {"method": "GET", "path": self.path, "headers": self.headers, "body": None}
        )
        project = {
            "id": PROJECT_ID,
            "title": "Field notes",
            "createdAt": "2026-10-14T10:00:00Z",
            "updatedAt": "2026-10-14T10:05:00Z",
        }
        if self.path == f"/api/v1/projects/{PROJECT_ID}":
            response = {
                **project,
                "document": {
                    "sections": [
                        {
                            "id": SECTION_ID,
                            "position": 0,
                            "audioAsset": {
                                "id": AUDIO_ASSET_ID,
                                "mediaType": "audio/webm;codecs=opus",
                                "isSealed": True,
                                "sealReason": "completed",
                                "sealedAt": "2026-10-14T10:04:00Z",
                                "frameCount": 2,
                                "observedDurationMs": 7350,
                                "storedBytes": 32768,
                            },
                        }
                    ]
                },
            }
        else:
            response = {"projects": [project]}
        payload = json.dumps(response).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_PATCH(self) -> None:  # noqa: N802
        length = int(self.headers["Content-Length"])
        body = json.loads(self.rfile.read(length))
        self.__class__.requests.append(
            {"method": "PATCH", "path": self.path, "headers": self.headers, "body": body}
        )
        response = {
            "id": PROJECT_ID,
            "title": body["title"],
            "createdAt": "2026-10-14T10:00:00Z",
            "updatedAt": "2026-10-14T10:10:00Z",
        }
        payload = json.dumps(response).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format: str, *args: object) -> None:
        pass


@contextmanager
def project_api() -> Iterator[tuple[str, type[ProjectApiHandler]]]:
    ProjectApiHandler.requests = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), ProjectApiHandler)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
        host, port = server.server_address
        yield f"http://{host}:{port}", ProjectApiHandler
    finally:
        server.shutdown()
        thread.join()
        server.server_close()


def run_cli(
    *arguments: str,
    api_url: str,
    extra_env: dict[str, str] | None = None,
    cwd: Path | None = None,
) -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy()
    environment.update(
        {
            "RDE_API_URL": api_url,
            "RDE_USERNAME": "developer",
            "RDE_PASSWORD": "secret-value",
        }
    )
    environment.update(extra_env or {})
    return subprocess.run(
        [sys.executable, "-m", "running_drafts_cli", *arguments],
        cwd=cwd or Path(__file__).parent.parent,
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )


class UploadApiHandler(BaseHTTPRequestHandler):
    requests: list[dict[str, object]] = []
    fail_first_frame_response = False
    failed_first_frame_response = False
    fail_all_frame_responses = False
    reject_sequence: int | None = None

    def _body(self) -> bytes:
        return self.rfile.read(int(self.headers.get("Content-Length", "0")))

    def _record(self, body: bytes) -> None:
        self.__class__.requests.append(
            {"method": self.command, "path": self.path, "headers": self.headers, "body": body}
        )

    def _json(self, status: int, value: dict[str, object]) -> None:
        payload = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_POST(self) -> None:  # noqa: N802
        body = self._body()
        self._record(body)
        if self.path == "/api/v1/projects":
            request = json.loads(body)
            self._json(
                201,
                {
                    "id": PROJECT_ID,
                    "title": request["title"],
                    "createdAt": "2026-10-14T10:00:00Z",
                    "updatedAt": "2026-10-14T10:00:00Z",
                },
            )
        elif self.path == f"/api/v1/projects/{PROJECT_ID}/sections":
            self._json(
                201,
                {"sectionId": SECTION_ID, "audioAssetId": AUDIO_ASSET_ID, "position": 0},
            )
        elif self.path == f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/complete":
            self._json(
                200,
                {
                    "id": AUDIO_ASSET_ID,
                    "mediaType": "audio/webm;codecs=opus",
                    "isSealed": True,
                    "sealReason": "completed",
                    "sealedAt": "2026-10-14T10:01:00Z",
                    "frameCount": 3,
                    "observedDurationMs": 12350,
                    "storedBytes": 50,
                },
            )
        elif self.path == f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/interrupt":
            self._json(
                200,
                {
                    "id": AUDIO_ASSET_ID,
                    "mediaType": "audio/webm;codecs=opus",
                    "isSealed": True,
                    "sealReason": "interrupted",
                    "sealedAt": "2026-10-14T10:01:00Z",
                    "frameCount": 1,
                    "observedDurationMs": 5000,
                    "storedBytes": 24,
                },
            )
        else:
            self.send_error(404)

    def do_PUT(self) -> None:  # noqa: N802
        body = self._body()
        self._record(body)
        sequence = int(self.path.rsplit("/", 1)[1])
        if sequence == self.__class__.reject_sequence:
            self._json(
                413,
                {
                    "type": "about:blank",
                    "title": "Audio frame too large",
                    "status": 413,
                    "detail": "The audio frame exceeds the configured size limit.",
                    "code": "frame_too_large",
                },
            )
            return
        if (
            sequence == 0
            and (
                self.__class__.fail_all_frame_responses or self.__class__.fail_first_frame_response
            )
            and not self.__class__.failed_first_frame_response
        ):
            if not self.__class__.fail_all_frame_responses:
                self.__class__.failed_first_frame_response = True
            self.connection.shutdown(socket.SHUT_RDWR)
            self.connection.close()
            return
        self._json(
            200,
            {
                "sequence": sequence,
                "acknowledged": True,
                "sha256": hashlib.sha256(body).hexdigest(),
                "nextExpectedSequence": sequence + 1,
            },
        )

    def log_message(self, format: str, *args: object) -> None:
        pass


@contextmanager
def upload_api(
    *,
    fail_first_frame_response: bool = False,
    fail_all_frame_responses: bool = False,
    reject_sequence: int | None = None,
) -> Iterator[tuple[str, type[UploadApiHandler]]]:
    UploadApiHandler.requests = []
    UploadApiHandler.fail_first_frame_response = fail_first_frame_response
    UploadApiHandler.failed_first_frame_response = False
    UploadApiHandler.fail_all_frame_responses = fail_all_frame_responses
    UploadApiHandler.reject_sequence = reject_sequence
    server = ThreadingHTTPServer(("127.0.0.1", 0), UploadApiHandler)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
        host, port = server.server_address
        yield f"http://{host}:{port}", UploadApiHandler
    finally:
        server.shutdown()
        thread.join()
        server.server_close()


class AuthenticationFailureHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:  # noqa: N802
        payload = json.dumps(
            {
                "type": "about:blank",
                "title": "Authentication required",
                "status": 401,
                "detail": "Supply valid Basic authentication credentials.",
                "code": "authentication_required",
            }
        ).encode()
        self.send_response(401)
        self.send_header("Content-Type", "application/problem+json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format: str, *args: object) -> None:
        pass


@contextmanager
def authentication_failure_api() -> Iterator[str]:
    server = ThreadingHTTPServer(("127.0.0.1", 0), AuthenticationFailureHandler)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
        host, port = server.server_address
        yield f"http://{host}:{port}"
    finally:
        server.shutdown()
        thread.join()
        server.server_close()


class DownloadApiHandler(BaseHTTPRequestHandler):
    requests: list[dict[str, object]] = []
    payload = b"original-audio"
    status = 200
    response_headers: dict[str, str] = {}
    problem: dict[str, object] | None = None
    truncate = False

    def do_GET(self) -> None:  # noqa: N802
        self.__class__.requests.append({"path": self.path, "headers": self.headers})
        if self.__class__.status != 200:
            payload = json.dumps(self.__class__.problem or {}).encode()
            self.send_response(self.__class__.status)
            self.send_header("Content-Type", "application/problem+json")
            self.send_header("Content-Length", str(len(payload)))
            for name, value in self.__class__.response_headers.items():
                self.send_header(name, value)
            self.end_headers()
            self.wfile.write(payload)
            return

        payload = self.__class__.payload
        self.send_response(200)
        for name, value in self.__class__.response_headers.items():
            self.send_header(name, value)
        self.end_headers()
        if self.__class__.truncate:
            self.wfile.write(f"{len(payload):X}\r\n".encode() + payload + b"\r\n")
        else:
            self.wfile.write(payload)
        self.wfile.flush()
        if self.__class__.truncate:
            self.connection.shutdown(socket.SHUT_RDWR)
            self.connection.close()

    def log_message(self, format: str, *args: object) -> None:
        pass


@contextmanager
def download_api(
    *,
    payload: bytes = b"original-audio",
    status: int = 200,
    headers: dict[str, str] | None = None,
    problem: dict[str, object] | None = None,
    truncate: bool = False,
) -> Iterator[tuple[str, type[DownloadApiHandler]]]:
    DownloadApiHandler.requests = []
    DownloadApiHandler.payload = payload
    DownloadApiHandler.status = status
    DownloadApiHandler.response_headers = headers or {}
    DownloadApiHandler.problem = problem
    DownloadApiHandler.truncate = truncate
    server = ThreadingHTTPServer(("127.0.0.1", 0), DownloadApiHandler)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
        host, port = server.server_address
        yield f"http://{host}:{port}", DownloadApiHandler
    finally:
        server.shutdown()
        thread.join()
        server.server_close()


class ConcurrentDownloadHandler(BaseHTTPRequestHandler):
    slow_started = Event()
    allow_slow_finish = Event()

    def do_GET(self) -> None:  # noqa: N802
        slow = "/slow/" in self.path
        payload = b"slow download wins last" if slow else b"fast download"
        self.send_response(200)
        self.send_header("Content-Type", "audio/webm;codecs=opus")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Content-Disposition", 'attachment; filename="shared.webm"')
        self.end_headers()
        if slow:
            midpoint = len(payload) // 2
            self.wfile.write(payload[:midpoint])
            self.wfile.flush()
            self.__class__.slow_started.set()
            if not self.__class__.allow_slow_finish.wait(timeout=10):
                return
            self.wfile.write(payload[midpoint:])
        else:
            self.wfile.write(payload)

    def log_message(self, format: str, *args: object) -> None:
        pass


@contextmanager
def concurrent_download_api() -> Iterator[tuple[str, type[ConcurrentDownloadHandler]]]:
    ConcurrentDownloadHandler.slow_started = Event()
    ConcurrentDownloadHandler.allow_slow_finish = Event()
    server = ThreadingHTTPServer(("127.0.0.1", 0), ConcurrentDownloadHandler)
    thread = Thread(target=server.serve_forever)
    thread.start()
    try:
        host, port = server.server_address
        yield f"http://{host}:{port}", ConcurrentDownloadHandler
    finally:
        ConcurrentDownloadHandler.allow_slow_finish.set()
        server.shutdown()
        thread.join()
        server.server_close()


def test_user_creates_a_project() -> None:
    with project_api() as (api_url, handler):
        result = run_cli("project", "create", "  Field notes  ", api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert result.stdout == f"Created project {PROJECT_ID}: Field notes\n"
    assert result.stderr == ""
    assert len(handler.requests) == 1
    request = handler.requests[0]
    assert request["path"] == "/api/v1/projects"
    assert (
        request["headers"]["Authorization"]
        == "Basic " + base64.b64encode(b"developer:secret-value").decode()
    )
    assert request["body"]["title"] == "Field notes"
    assert UUID(request["body"]["idempotencyKey"]).version == 4


def test_user_lists_projects() -> None:
    with project_api() as (api_url, handler):
        result = run_cli("project", "list", api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert result.stdout == f"{PROJECT_ID}  Field notes  updated 2026-10-14T10:05:00Z\n"
    assert result.stderr == ""
    assert [(request["method"], request["path"]) for request in handler.requests] == [
        ("GET", "/api/v1/projects")
    ]


def test_user_shows_a_project_and_its_audio() -> None:
    with project_api() as (api_url, handler):
        result = run_cli("project", "show", PROJECT_ID, api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert result.stdout == (
        f"Project {PROJECT_ID}: Field notes\n"
        f"Section 1  {AUDIO_ASSET_ID}  completed  2 frames  7350 ms  32768 bytes\n"
    )
    assert result.stderr == ""
    assert [(request["method"], request["path"]) for request in handler.requests] == [
        ("GET", f"/api/v1/projects/{PROJECT_ID}")
    ]


def test_user_renames_a_project() -> None:
    with project_api() as (api_url, handler):
        result = run_cli("project", "rename", PROJECT_ID, "  Revised notes  ", api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert result.stdout == f"Renamed project {PROJECT_ID}: Revised notes\n"
    assert result.stderr == ""
    assert handler.requests[0]["method"] == "PATCH"
    assert handler.requests[0]["path"] == f"/api/v1/projects/{PROJECT_ID}"
    assert handler.requests[0]["body"] == {"title": "Revised notes"}


def test_user_uploads_a_file_to_a_new_project(tmp_path: Path) -> None:
    recording = tmp_path / "check-english.wav"
    recording.write_bytes(b"source audio")

    with upload_api() as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--ffmpeg",
            str(FAKE_FFMPEG),
            api_url=api_url,
        )

    assert result.returncode == 0, result.stderr
    assert result.stdout == (
        f"Uploaded check-english.wav to project {PROJECT_ID}, section 1 "
        f"({AUDIO_ASSET_ID}): 3 frames, 12350 ms\n"
    )
    assert result.stderr == (
        "rde: ffmpeg: starting conversion of check-english.wav\n"
        "rde: ffmpeg: finished conversion: 3 frames, 12350 ms\n"
    )
    requests = handler.requests
    assert [request["method"] for request in requests] == [
        "POST",
        "POST",
        "PUT",
        "PUT",
        "PUT",
        "POST",
    ]
    assert json.loads(requests[0]["body"])["title"] == "check-english"
    section_request = json.loads(requests[1]["body"])
    assert section_request["mediaType"] == "audio/webm;codecs=opus"
    assert UUID(section_request["idempotencyKey"]).version == 4
    assert [request["body"] for request in requests[2:5]] == [
        b"webm-headerfirst-cluster",
        b"second-cluster",
        b"last-cluster",
    ]
    assert [
        (request["headers"]["X-Frame-Start-Ms"], request["headers"]["X-Frame-End-Ms"])
        for request in requests[2:5]
    ] == [("0", "5000"), ("5000", "10000"), ("10000", "12350")]
    assert json.loads(requests[5]["body"]) == {"frameCount": 3}


def test_user_overrides_the_new_project_title(tmp_path: Path) -> None:
    recording = tmp_path / "ignored-name.wav"
    recording.write_bytes(b"source audio")

    with upload_api() as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--title",
            "  Walk notes  ",
            "--ffmpeg",
            str(FAKE_FFMPEG),
            api_url=api_url,
        )

    assert result.returncode == 0, result.stderr
    assert json.loads(handler.requests[0]["body"])["title"] == "Walk notes"


def test_user_uploads_a_file_as_a_new_section_of_an_existing_project(tmp_path: Path) -> None:
    recording = tmp_path / "second thought.wav"
    recording.write_bytes(b"source audio")

    with upload_api() as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--project",
            PROJECT_ID,
            "--ffmpeg",
            str(FAKE_FFMPEG),
            api_url=api_url,
        )

    assert result.returncode == 0, result.stderr
    assert result.stdout.startswith(
        f"Uploaded second thought.wav to project {PROJECT_ID}, section 1"
    )
    assert handler.requests[0]["path"] == f"/api/v1/projects/{PROJECT_ID}/sections"
    assert all(request["path"] != "/api/v1/projects" for request in handler.requests)


def test_uncertain_frame_response_retries_identical_request(tmp_path: Path) -> None:
    recording = tmp_path / "retry.wav"
    recording.write_bytes(b"source audio")

    with upload_api(fail_first_frame_response=True) as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--ffmpeg",
            str(FAKE_FFMPEG),
            api_url=api_url,
        )

    assert result.returncode == 0, result.stderr
    frame_zero_attempts = [
        request
        for request in handler.requests
        if request["path"] == f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/frames/0"
    ]
    assert len(frame_zero_attempts) == 2
    assert frame_zero_attempts[0]["body"] == frame_zero_attempts[1]["body"]
    assert frame_zero_attempts[0]["headers"]["X-Frame-Start-Ms"] == "0"
    assert frame_zero_attempts[1]["headers"]["X-Frame-Start-Ms"] == "0"
    assert frame_zero_attempts[0]["headers"]["X-Frame-End-Ms"] == "5000"
    assert frame_zero_attempts[1]["headers"]["X-Frame-End-Ms"] == "5000"


def test_uncertain_frame_response_exhausts_three_attempts_then_interrupts(
    tmp_path: Path,
) -> None:
    recording = tmp_path / "retry-exhausted.wav"
    recording.write_bytes(b"source audio")

    with upload_api(fail_all_frame_responses=True) as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--ffmpeg",
            str(FAKE_FFMPEG),
            api_url=api_url,
        )

    assert result.returncode == 1
    frame_zero_attempts = [
        request
        for request in handler.requests
        if request["path"] == f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/frames/0"
    ]
    assert len(frame_zero_attempts) == 3
    assert len({request["body"] for request in frame_zero_attempts}) == 1
    assert handler.requests[-1]["path"] == (f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/interrupt")


def test_permanent_upload_failure_interrupts_the_created_asset(tmp_path: Path) -> None:
    recording = tmp_path / "too-large.wav"
    recording.write_bytes(b"source audio")

    with upload_api(reject_sequence=1) as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--ffmpeg",
            str(FAKE_FFMPEG),
            api_url=api_url,
        )

    assert result.returncode == 1
    assert result.stdout == ""
    assert f"project {PROJECT_ID}, audio {AUDIO_ASSET_ID}" in result.stderr
    assert "sealed as interrupted" in result.stderr
    assert "secret-value" not in result.stderr
    assert [request["path"] for request in handler.requests[-2:]] == [
        f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/frames/1",
        f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/interrupt",
    ]
    assert handler.requests[-1]["body"] == b""


def test_api_problem_is_actionable_and_redacts_credentials() -> None:
    with authentication_failure_api() as api_url:
        result = run_cli("project", "list", api_url=api_url)

    assert result.returncode == 1
    assert result.stdout == ""
    assert result.stderr == (
        "rde: error: authentication_required: Supply valid Basic authentication credentials.\n"
    )
    assert "secret-value" not in result.stderr


def test_conversion_failure_precedes_server_mutation_and_cleans_temporary_files(
    tmp_path: Path,
) -> None:
    recording = tmp_path / "broken.wav"
    recording.write_bytes(b"not audio")
    temporary_log = tmp_path / "temporary-path.txt"

    with upload_api() as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--ffmpeg",
            str(FAKE_FFMPEG),
            api_url=api_url,
            extra_env={"FAKE_FFMPEG_TEMP_LOG": str(temporary_log)},
        )

    assert result.returncode == 1
    assert result.stdout == ""
    assert result.stderr == (
        "rde: ffmpeg: starting conversion of broken.wav\n"
        "rde: error: ffmpeg conversion failed: input is not decodable\n"
    )
    assert handler.requests == []
    assert not Path(temporary_log.read_text()).exists()


def test_missing_ffmpeg_precedes_server_mutation(tmp_path: Path) -> None:
    recording = tmp_path / "recording.wav"
    recording.write_bytes(b"source audio")

    with upload_api() as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--ffmpeg",
            str(tmp_path / "missing-ffmpeg"),
            api_url=api_url,
        )

    assert result.returncode == 1
    assert "could not run ffmpeg" in result.stderr
    assert handler.requests == []


def test_user_configures_fixed_frame_duration(tmp_path: Path) -> None:
    recording = tmp_path / "short-frames.wav"
    recording.write_bytes(b"source audio")

    with upload_api() as (api_url, handler):
        result = run_cli(
            "upload",
            str(recording),
            "--frame-duration",
            "2",
            "--ffmpeg",
            str(FAKE_FFMPEG),
            api_url=api_url,
        )

    assert result.returncode == 0, result.stderr
    frames = [request for request in handler.requests if request["method"] == "PUT"]
    assert len(frames) == 7
    assert [
        (request["headers"]["X-Frame-Start-Ms"], request["headers"]["X-Frame-End-Ms"])
        for request in frames
    ] == [
        ("0", "2000"),
        ("2000", "4000"),
        ("4000", "6000"),
        ("6000", "8000"),
        ("8000", "10000"),
        ("10000", "12000"),
        ("12000", "12350"),
    ]


def test_user_downloads_under_the_unicode_server_filename(tmp_path: Path) -> None:
    payload = b"webm-header-and-clusters"
    disposition = (
        'attachment; filename="download-section-1.webm"; '
        "filename*=UTF-8''%C5%BDlut%C3%BD-%E6%9D%B1%E4%BA%AC-section-1.webm"
    )
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": str(len(payload)),
            "Content-Disposition": disposition,
        },
    ) as (api_url, handler):
        result = run_cli("download", AUDIO_ASSET_ID, api_url=api_url, cwd=tmp_path)

    destination = tmp_path / "Žlutý-東京-section-1.webm"
    assert result.returncode == 0, result.stderr
    assert result.stdout == ""
    assert result.stderr == ""
    assert destination.read_bytes() == payload
    assert handler.requests[0]["path"] == (f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/download")
    assert (
        handler.requests[0]["headers"]["Authorization"]
        == "Basic " + base64.b64encode(b"developer:secret-value").decode()
    )


def test_download_uses_ascii_filename_fallback_and_supports_empty_audio(
    tmp_path: Path,
) -> None:
    with download_api(
        payload=b"",
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": "0",
            "Content-Disposition": 'attachment; filename="empty-section-1.webm"',
        },
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, api_url=api_url, cwd=tmp_path)

    assert result.returncode == 0, result.stderr
    assert result.stdout == ""
    assert result.stderr == ""
    assert (tmp_path / "empty-section-1.webm").read_bytes() == b""


def test_download_warns_and_uses_ascii_fallback_for_invalid_unicode_filename(
    tmp_path: Path,
) -> None:
    payload = b"audio"
    disposition = (
        "attachment; filename=\"fallback-section-1.webm\"; filename*=UTF-8''invalid%ZZ.webm"
    )
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": str(len(payload)),
            "Content-Disposition": disposition,
        },
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, api_url=api_url, cwd=tmp_path)

    assert result.returncode == 0, result.stderr
    assert "invalid Unicode filename" in result.stderr
    assert (tmp_path / "fallback-section-1.webm").read_bytes() == payload


def test_explicit_output_silently_replaces_existing_file(tmp_path: Path) -> None:
    destination = tmp_path / "chosen.mp3"
    destination.write_bytes(b"old data")
    payload = b"new original audio"
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": str(len(payload)),
            "Content-Disposition": 'attachment; filename="ignored.webm"',
        },
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", str(destination), api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert result.stdout == ""
    assert result.stderr == ""
    assert destination.read_bytes() == payload


def test_download_warns_but_saves_for_permissive_response_metadata(
    tmp_path: Path,
) -> None:
    destination = tmp_path / "diagnostic.bin"
    payload = b"server bytes"
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "application/octet-stream",
            "Content-Disposition": 'attachment; filename="ignored.webm"',
        },
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", str(destination), api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert result.stdout == ""
    assert "rde: warning:" in result.stderr
    assert "Content-Type" in result.stderr
    assert "Content-Length" in result.stderr
    assert destination.read_bytes() == payload


def test_download_warns_but_saves_for_malformed_content_length(tmp_path: Path) -> None:
    destination = tmp_path / "malformed-length.webm"
    payload = b"all server bytes"
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": "not-a-number",
            "Content-Disposition": 'attachment; filename="ignored.webm"',
        },
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", str(destination), api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert "invalid Content-Length 'not-a-number'" in result.stderr
    assert destination.read_bytes() == payload


def test_unsafe_server_filename_warns_and_uses_asset_id(tmp_path: Path) -> None:
    payload = b"audio"
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": str(len(payload)),
            "Content-Disposition": 'attachment; filename="../../outside.webm"',
        },
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, api_url=api_url, cwd=tmp_path)

    assert result.returncode == 0, result.stderr
    assert result.stdout == ""
    assert "rde: warning:" in result.stderr
    assert "unsafe" in result.stderr
    assert (tmp_path / f"{AUDIO_ASSET_ID}.webm").read_bytes() == payload
    assert not (tmp_path.parent / "outside.webm").exists()


def test_interrupted_download_retains_partial_and_preserves_destination(
    tmp_path: Path,
) -> None:
    destination = tmp_path / "recording.webm"
    destination.write_bytes(b"previous complete download")
    Path(f"{destination}.partial").write_bytes(b"older partial download")
    payload = b"partial audio bytes"
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Transfer-Encoding": "chunked",
            "Content-Disposition": 'attachment; filename="recording.webm"',
        },
        truncate=True,
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", str(destination), api_url=api_url)

    assert result.returncode == 1
    assert result.stdout == ""
    assert "download interrupted" in result.stderr
    assert "partial data saved" in result.stderr
    assert "secret-value" not in result.stderr
    assert destination.read_bytes() == b"previous complete download"
    assert Path(f"{destination}.partial").read_bytes() == payload


def test_clean_content_length_mismatch_warns_and_publishes_all_bytes(
    tmp_path: Path,
) -> None:
    destination = tmp_path / "length-mismatch.webm"
    payload = b"all bytes before a clean end"
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": str(len(payload) + 50),
            "Content-Disposition": 'attachment; filename="ignored.webm"',
        },
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", str(destination), api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert result.stderr == (
        f"rde: warning: server declared Content-Length {len(payload) + 50}, but "
        f"{len(payload)} bytes were received; saved to {destination}\n"
    )
    assert destination.read_bytes() == payload


def test_download_preserves_raw_encoded_response_bytes(tmp_path: Path) -> None:
    destination = tmp_path / "encoded.webm"
    payload = b"not-decoded-gzip-bytes"
    with download_api(
        payload=payload,
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": str(len(payload)),
            "Content-Encoding": "gzip",
            "Content-Disposition": 'attachment; filename="encoded.webm"',
        },
    ) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", str(destination), api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert destination.read_bytes() == payload


def test_download_problem_is_reported_without_an_artifact(tmp_path: Path) -> None:
    problem = {
        "type": "about:blank",
        "title": "Command conflicts with existing state",
        "status": 409,
        "detail": "The audio asset must be sealed before it can be downloaded.",
        "code": "asset_not_sealed",
    }
    destination = tmp_path / "open.webm"
    with download_api(status=409, problem=problem) as (api_url, _handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", str(destination), api_url=api_url)

    assert result.returncode == 1
    assert result.stdout == ""
    assert result.stderr == (
        "rde: error: asset_not_sealed: "
        "The audio asset must be sealed before it can be downloaded.\n"
    )
    assert not destination.exists()
    assert list(tmp_path.iterdir()) == []


def test_download_rejects_stdout_before_contacting_server(tmp_path: Path) -> None:
    with download_api() as (api_url, handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", "-", api_url=api_url, cwd=tmp_path)

    assert result.returncode == 1
    assert result.stdout == ""
    assert result.stderr == "rde: error: --output - is not supported\n"
    assert handler.requests == []


def test_download_treats_asset_id_as_an_opaque_path_segment(tmp_path: Path) -> None:
    destination = tmp_path / "opaque.webm"
    with download_api(
        headers={
            "Content-Type": "audio/webm;codecs=opus",
            "Content-Length": "14",
            "Content-Disposition": 'attachment; filename="opaque.webm"',
        },
    ) as (api_url, handler):
        result = run_cli("download", "not/a UUID", "--output", str(destination), api_url=api_url)

    assert result.returncode == 0, result.stderr
    assert handler.requests[0]["path"] == ("/api/v1/audio-assets/not%2Fa%20UUID/download")


def test_download_does_not_follow_redirects(tmp_path: Path) -> None:
    destination = tmp_path / "redirect.webm"
    with download_api(
        status=302,
        headers={"Location": f"/api/v1/audio-assets/{AUDIO_ASSET_ID}/download"},
    ) as (api_url, handler):
        result = run_cli("download", AUDIO_ASSET_ID, "--output", str(destination), api_url=api_url)

    assert result.returncode == 1
    assert len(handler.requests) == 1
    assert not destination.exists()


def test_concurrent_download_that_finishes_last_replaces_the_destination(
    tmp_path: Path,
) -> None:
    destination = tmp_path / "shared.webm"
    with (
        concurrent_download_api() as (api_url, handler),
        ThreadPoolExecutor(max_workers=1) as executor,
    ):
        slow_result = executor.submit(
            run_cli,
            "download",
            "slow",
            "--output",
            str(destination),
            api_url=api_url,
        )
        assert handler.slow_started.wait(timeout=10)
        fast_result = run_cli("download", "fast", "--output", str(destination), api_url=api_url)
        assert fast_result.returncode == 0, fast_result.stderr
        assert destination.read_bytes() == b"fast download"
        handler.allow_slow_finish.set()
        completed_slow_result = slow_result.result(timeout=10)

    assert completed_slow_result.returncode == 0, completed_slow_result.stderr
    assert destination.read_bytes() == b"slow download wins last"
    assert list(tmp_path.glob("*.partial")) == []

from __future__ import annotations

import base64
import hashlib
import json
import os
import socket
import subprocess
import sys
from collections.abc import Iterator
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
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
    *arguments: str, api_url: str, extra_env: dict[str, str] | None = None
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
        cwd=Path(__file__).parent.parent,
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
    assert result.stderr == ""
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
    assert result.stderr == "rde: error: ffmpeg conversion failed: input is not decodable\n"
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

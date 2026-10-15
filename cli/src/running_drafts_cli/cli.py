from __future__ import annotations

import argparse
import hashlib
import os
import unicodedata
from collections.abc import Sequence
from pathlib import Path
from uuid import uuid4

import httpx

from running_drafts_cli.audio import AudioFrame, AudioPreparationError, prepare_audio

MEDIA_TYPE = "audio/webm;codecs=opus"


class UploadError(Exception):
    """An upload failed after its server-side asset was created."""


def _error_message(error: Exception) -> str:
    if isinstance(error, httpx.HTTPStatusError):
        try:
            problem = error.response.json()
        except ValueError:
            problem = None
        if isinstance(problem, dict):
            code = problem.get("code")
            detail = problem.get("detail")
            if isinstance(code, str) and isinstance(detail, str):
                return f"{code}: {detail}"
    return str(error)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="rde")
    parser.add_argument("--api-url", default=os.environ.get("RDE_API_URL"))
    parser.add_argument("--username", default=os.environ.get("RDE_USERNAME"))
    parser.set_defaults(password=os.environ.get("RDE_PASSWORD"))

    commands = parser.add_subparsers(dest="command", required=True)
    project = commands.add_parser("project")
    project_commands = project.add_subparsers(dest="project_command", required=True)
    create = project_commands.add_parser("create")
    create.add_argument("title")
    project_commands.add_parser("list")
    show = project_commands.add_parser("show")
    show.add_argument("project_id")
    rename = project_commands.add_parser("rename")
    rename.add_argument("project_id")
    rename.add_argument("title")
    upload = commands.add_parser("upload")
    upload.add_argument("file", type=Path)
    upload.add_argument("--project")
    upload.add_argument("--title")
    upload.add_argument("--ffmpeg", default="ffmpeg")
    upload.add_argument("--frame-duration", type=float, default=5.0, metavar="SECONDS")
    return parser


def _required(value: str | None, name: str) -> str:
    if value:
        return value
    raise ValueError(f"{name} is required")


def _create_project(arguments: argparse.Namespace) -> None:
    title = unicodedata.normalize("NFC", arguments.title).strip()
    response = httpx.post(
        f"{arguments.api_url.rstrip('/')}/api/v1/projects",
        auth=(arguments.username, arguments.password),
        json={"idempotencyKey": str(uuid4()), "title": title},
        timeout=30,
    )
    response.raise_for_status()
    project = response.json()
    print(f"Created project {project['id']}: {project['title']}")


def _list_projects(arguments: argparse.Namespace) -> None:
    response = httpx.get(
        f"{arguments.api_url.rstrip('/')}/api/v1/projects",
        auth=(arguments.username, arguments.password),
        timeout=30,
    )
    response.raise_for_status()
    for project in response.json()["projects"]:
        print(f"{project['id']}  {project['title']}  updated {project['updatedAt']}")


def _show_project(arguments: argparse.Namespace) -> None:
    response = httpx.get(
        f"{arguments.api_url.rstrip('/')}/api/v1/projects/{arguments.project_id}",
        auth=(arguments.username, arguments.password),
        timeout=30,
    )
    response.raise_for_status()
    project = response.json()
    print(f"Project {project['id']}: {project['title']}")
    for section in project["document"]["sections"]:
        asset = section["audioAsset"]
        state = asset["sealReason"] if asset["isSealed"] else "open"
        print(
            f"Section {section['position'] + 1}  {asset['id']}  {state}  "
            f"{asset['frameCount']} frames  {asset['observedDurationMs']} ms  "
            f"{asset['storedBytes']} bytes"
        )


def _rename_project(arguments: argparse.Namespace) -> None:
    title = unicodedata.normalize("NFC", arguments.title).strip()
    response = httpx.patch(
        f"{arguments.api_url.rstrip('/')}/api/v1/projects/{arguments.project_id}",
        auth=(arguments.username, arguments.password),
        json={"title": title},
        timeout=30,
    )
    response.raise_for_status()
    project = response.json()
    print(f"Renamed project {project['id']}: {project['title']}")


def _post_json(
    arguments: argparse.Namespace, path: str, payload: dict[str, object]
) -> httpx.Response:
    response = httpx.post(
        f"{arguments.api_url.rstrip('/')}{path}",
        auth=(arguments.username, arguments.password),
        json=payload,
        timeout=30,
    )
    response.raise_for_status()
    return response


def _upload_frame(arguments: argparse.Namespace, audio_asset_id: str, frame: AudioFrame) -> None:
    response: httpx.Response | None = None
    last_error: httpx.TransportError | None = None
    for _attempt in range(3):
        try:
            response = httpx.put(
                f"{arguments.api_url.rstrip('/')}/api/v1/audio-assets/"
                f"{audio_asset_id}/frames/{frame.sequence}",
                auth=(arguments.username, arguments.password),
                headers={
                    "Content-Type": MEDIA_TYPE,
                    "X-Frame-Start-Ms": str(frame.start_ms),
                    "X-Frame-End-Ms": str(frame.end_ms),
                },
                content=frame.payload,
                timeout=30,
            )
            break
        except httpx.TransportError as error:
            last_error = error
    if response is None:
        assert last_error is not None
        raise last_error
    response.raise_for_status()
    acknowledgement = response.json()
    expected_sha256 = hashlib.sha256(frame.payload).hexdigest()
    if acknowledgement != {
        "sequence": frame.sequence,
        "acknowledged": True,
        "sha256": expected_sha256,
        "nextExpectedSequence": frame.sequence + 1,
    }:
        raise ValueError(f"invalid acknowledgement for frame {frame.sequence}")


def _interrupt(arguments: argparse.Namespace, audio_asset_id: str) -> None:
    response = httpx.post(
        f"{arguments.api_url.rstrip('/')}/api/v1/audio-assets/{audio_asset_id}/interrupt",
        auth=(arguments.username, arguments.password),
        content=b"",
        timeout=30,
    )
    response.raise_for_status()


def _upload(arguments: argparse.Namespace) -> None:
    if arguments.frame_duration <= 0:
        raise ValueError("frame duration must be greater than zero")
    if arguments.project and arguments.title:
        raise ValueError("--title cannot be used with --project")
    frame_duration_ms = round(arguments.frame_duration * 1000)
    if frame_duration_ms <= 0:
        raise ValueError("frame duration is too small")

    with prepare_audio(
        arguments.file, ffmpeg=arguments.ffmpeg, frame_duration_ms=frame_duration_ms
    ) as audio:
        if arguments.project:
            project_id = arguments.project
        else:
            title = arguments.title if arguments.title is not None else arguments.file.stem
            title = unicodedata.normalize("NFC", title).strip()
            project = _post_json(
                arguments,
                "/api/v1/projects",
                {"idempotencyKey": str(uuid4()), "title": title},
            ).json()
            project_id = project["id"]

        section = _post_json(
            arguments,
            f"/api/v1/projects/{project_id}/sections",
            {"idempotencyKey": str(uuid4()), "mediaType": MEDIA_TYPE},
        ).json()
        audio_asset_id = section["audioAssetId"]

        try:
            for frame in audio.frames:
                _upload_frame(arguments, audio_asset_id, frame.read())

            _post_json(
                arguments,
                f"/api/v1/audio-assets/{audio_asset_id}/complete",
                {"frameCount": len(audio.frames)},
            )
        except (ValueError, httpx.HTTPError) as error:
            try:
                _interrupt(arguments, audio_asset_id)
                disposition = "asset sealed as interrupted"
            except httpx.HTTPError:
                disposition = "asset could not be sealed as interrupted"
            raise UploadError(
                f"upload failed for project {project_id}, audio {audio_asset_id}; "
                f"{disposition}: {_error_message(error)}"
            ) from None
        print(
            f"Uploaded {arguments.file.name} to project {project_id}, "
            f"section {section['position'] + 1} ({audio_asset_id}): "
            f"{len(audio.frames)} frames, {audio.duration_ms} ms"
        )


def main(argv: Sequence[str] | None = None) -> int:
    parser = _parser()
    arguments = parser.parse_args(argv)
    try:
        arguments.api_url = _required(arguments.api_url, "API URL")
        arguments.username = _required(arguments.username, "username")
        arguments.password = _required(arguments.password, "password")
        if arguments.command == "project" and arguments.project_command == "create":
            _create_project(arguments)
        elif arguments.command == "project" and arguments.project_command == "list":
            _list_projects(arguments)
        elif arguments.command == "project" and arguments.project_command == "show":
            _show_project(arguments)
        elif arguments.command == "project" and arguments.project_command == "rename":
            _rename_project(arguments)
        elif arguments.command == "upload":
            _upload(arguments)
        return 0
    except (AudioPreparationError, UploadError, ValueError, httpx.HTTPError) as error:
        parser.exit(1, f"rde: error: {_error_message(error)}\n")

from __future__ import annotations

import base64
import http.client
import json
import os
import re
import sys
import tempfile
import unicodedata
from email.message import Message
from pathlib import Path, PureWindowsPath
from typing import BinaryIO
from urllib.parse import quote, unquote_to_bytes, urlsplit

MEDIA_TYPE = "audio/webm;codecs=opus"


class DownloadError(Exception):
    """An audio download could not be completed."""


class DownloadCancelled(DownloadError):
    """The user interrupted an audio download."""


def _warning(message: str) -> None:
    print(f"rde: warning: {message}", file=sys.stderr)


def _safe_filename(filename: str) -> bool:
    if not filename or filename in {".", ".."}:
        return False
    if "/" in filename or "\\" in filename:
        return False
    if PureWindowsPath(filename).drive:
        return False
    return not any(unicodedata.category(character) == "Cc" for character in filename)


def _unicode_filename(value: str) -> tuple[str | None, bool]:
    match = re.search(r"(?:^|;)\s*filename\*\s*=\s*([^;]*)", value, re.IGNORECASE)
    if match is None:
        return None, False
    parameter = match.group(1).strip()
    if len(parameter) >= 2 and parameter[0] == parameter[-1] == '"':
        parameter = parameter[1:-1]
    try:
        charset, _language, encoded = parameter.split("'", 2)
        if charset.lower() != "utf-8":
            return None, True
        if re.search(r"%(?![0-9A-Fa-f]{2})", encoded):
            return None, True
        filename = unquote_to_bytes(encoded).decode("utf-8")
    except (UnicodeDecodeError, UnicodeEncodeError, ValueError):
        return None, True
    return filename, False


def _content_disposition_filenames(
    value: str | None,
) -> tuple[str | None, str | None, bool]:
    if value is None:
        return None, None, False
    unicode_filename, invalid_unicode = _unicode_filename(value)
    message = Message()
    message["Content-Disposition"] = value
    ascii_filename: str | None = None
    for name, parameter in message.get_params(header="content-disposition", unquote=True)[1:]:
        if name.lower() != "filename":
            continue
        if isinstance(parameter, str) and parameter.isascii() and ascii_filename is None:
            ascii_filename = parameter
    return unicode_filename, ascii_filename, invalid_unicode


def _fallback_filename(audio_asset_id: str) -> str:
    encoded = quote(audio_asset_id, safe="")
    return f"{encoded or 'audio-asset'}.webm"


def _server_destination(headers: http.client.HTTPMessage, audio_asset_id: str) -> Path:
    unicode_filename, ascii_filename, invalid_unicode = _content_disposition_filenames(
        headers.get("content-disposition")
    )
    if invalid_unicode:
        _warning("server supplied an invalid Unicode filename; ignoring it")
    for label, filename in (
        ("Unicode", unicode_filename),
        ("ASCII", ascii_filename),
    ):
        if filename is None:
            continue
        if _safe_filename(filename):
            return Path(filename)
        _warning(f"server supplied an unsafe {label} filename; ignoring it")
    fallback = _fallback_filename(audio_asset_id)
    _warning(f"server supplied no safe download filename; using {fallback}")
    return Path(fallback)


def _expected_length(headers: http.client.HTTPMessage) -> int | None:
    value = headers.get("content-length")
    if value is None:
        _warning("server supplied no Content-Length; saving received bytes")
        return None
    try:
        length = int(value)
    except ValueError:
        _warning(f"server supplied invalid Content-Length {value!r}; saving received bytes")
        return None
    if length < 0:
        _warning(f"server supplied invalid Content-Length {value!r}; saving received bytes")
        return None
    return length


def _check_media_type(headers: http.client.HTTPMessage) -> None:
    media_type = headers.get("content-type")
    if media_type is None or media_type.lower().replace(" ", "") != MEDIA_TYPE:
        _warning(
            f"server supplied Content-Type {media_type!r}, expected {MEDIA_TYPE}; "
            "saving received bytes"
        )


def _temporary_file(destination: Path) -> tuple[Path, BinaryIO]:
    try:
        descriptor, name = tempfile.mkstemp(
            prefix=f".{destination.name}.",
            suffix=".partial",
            dir=destination.parent,
        )
    except OSError as error:
        raise DownloadError(f"could not create download file: {error}") from None
    try:
        file = os.fdopen(descriptor, "wb")
    except OSError as error:
        os.close(descriptor)
        raise DownloadError(f"could not open download file {name}: {error}") from None
    return Path(name), file


def _retain_partial(temporary: Path, destination: Path) -> Path:
    partial = Path(f"{destination}.partial")
    try:
        os.replace(temporary, partial)
    except OSError:
        return temporary
    return partial


def _interrupted_message(
    error: BaseException, received: int, temporary: Path, destination: Path
) -> str:
    retained = _retain_partial(temporary, destination)
    return f"download interrupted after {received} bytes; partial data saved to {retained}: {error}"


def _connection(url: str) -> tuple[http.client.HTTPConnection, str]:
    # http.client leaves malformed Content-Length values available to the caller.
    # HTTPX rejects them while parsing, before this CLI can warn and retain the bytes.
    try:
        parsed = urlsplit(url)
        host = parsed.hostname
        port = parsed.port
    except ValueError as error:
        raise DownloadError(f"invalid API URL: {error}") from None
    if host is None:
        raise DownloadError("invalid API URL: host is required")
    if parsed.scheme == "http":
        connection: http.client.HTTPConnection = http.client.HTTPConnection(host, port)
    elif parsed.scheme == "https":
        connection = http.client.HTTPSConnection(host, port)
    else:
        raise DownloadError(f"invalid API URL scheme: {parsed.scheme or '(missing)'}")
    target = parsed.path or "/"
    if parsed.query:
        target = f"{target}?{parsed.query}"
    return connection, target


def _basic_authorization(username: str, password: str) -> str:
    credentials = base64.b64encode(f"{username}:{password}".encode()).decode("ascii")
    return f"Basic {credentials}"


def _raise_response_error(response: http.client.HTTPResponse) -> None:
    try:
        body = response.read()
        problem = json.loads(body)
    except (http.client.HTTPException, OSError, UnicodeDecodeError, ValueError):
        problem = None
    if isinstance(problem, dict):
        code = problem.get("code")
        detail = problem.get("detail")
        if isinstance(code, str) and isinstance(detail, str):
            raise DownloadError(f"{code}: {detail}")
    raise DownloadError(f"HTTP {response.status}: {response.reason}")


def download_audio(
    *,
    api_url: str,
    username: str,
    password: str,
    audio_asset_id: str,
    output: Path | None,
) -> None:
    if output == Path("-"):
        raise DownloadError("--output - is not supported")
    if output is not None:
        if not output.parent.is_dir():
            raise DownloadError(f"output directory does not exist: {output.parent}")
        if output.is_dir():
            raise DownloadError(f"output is a directory: {output}")

    encoded_id = quote(audio_asset_id, safe="")
    url = f"{api_url.rstrip('/')}/api/v1/audio-assets/{encoded_id}/download"
    connection, target = _connection(url)
    try:
        connection.request(
            "GET",
            target,
            headers={"Authorization": _basic_authorization(username, password)},
        )
        response = connection.getresponse()
    except (http.client.HTTPException, OSError) as error:
        connection.close()
        raise DownloadError(str(error)) from None
    try:
        if response.status != 200:
            _raise_response_error(response)

        destination = (
            output if output is not None else _server_destination(response.headers, audio_asset_id)
        )
        if not destination.parent.is_dir():
            raise DownloadError(f"output directory does not exist: {destination.parent}")
        if destination.is_dir():
            raise DownloadError(f"output is a directory: {destination}")
        expected_length = _expected_length(response.headers)
        _check_media_type(response.headers)
        temporary, file = _temporary_file(destination)
        received = 0
        try:
            with file:
                while chunk := response.read1(64 * 1024):
                    received += len(chunk)
                    file.write(chunk)
        except KeyboardInterrupt as error:
            message = _interrupted_message(error, received, temporary, destination)
            raise DownloadCancelled(message) from None
        except (OSError, http.client.HTTPException) as error:
            message = _interrupted_message(error, received, temporary, destination)
            raise DownloadError(message) from None

        try:
            os.replace(temporary, destination)
        except OSError as error:
            message = _interrupted_message(error, received, temporary, destination)
            raise DownloadError(message) from None

        if expected_length is not None and received != expected_length:
            _warning(
                f"server declared Content-Length {expected_length}, but {received} bytes "
                f"were received; saved to {destination}"
            )
    finally:
        response.close()
        connection.close()

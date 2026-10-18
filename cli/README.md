# Running Drafts CLI

`rde` is the product-level command-line client for Running Drafts. It manages
Projects through the HTTP API, uploads an existing audio file as a new Section,
and downloads sealed original audio. It does not record from a microphone.

## Requirements

- Python 3.11 or newer
- [`uv`](https://docs.astral.sh/uv/)
- `ffmpeg` with the `libopus` encoder and `webm_chunk` muxer
- A migrated Running Drafts backend and an authenticated user

## Install

From this directory:

```sh
uv tool install .
rde --help
```

During development, run commands without installing the tool:

```sh
uv sync --frozen
uv run rde --help
```

## Connect

The initial client uses three environment variables. Productized connection
profiles and credential handling are deferred to editor issue #89.

```sh
export RDE_API_URL=http://127.0.0.1:8000
export RDE_USERNAME=developer
export RDE_PASSWORD='your-development-password'
```

The API URL and username may instead be passed as global options before the
command:

```sh
rde --api-url http://127.0.0.1:8000 --username developer project list
```

The password has no command-line option, so it is not placed in shell history
or the process list.

## Projects

```sh
rde project create "October notes"
rde project list
rde project show PROJECT_ID
rde project rename PROJECT_ID "Revised title"
```

## Upload audio

Create a Project named from the input filename and upload one Section:

```sh
rde upload recording.wav
```

Override the new Project title, or append a new Section to an existing Project:

```sh
rde upload recording.m4a --title "Walk notes"
rde upload another-recording.wav --project PROJECT_ID
```

`rde` asks `ffmpeg` to transcode the complete input timeline to WebM/Opus before
creating server resources. It then uploads deterministic five-second transport
frames in order and seals the Audio Asset as completed. Select a different fixed
duration or executable when needed:

```sh
rde upload recording.wav --frame-duration 2 --ffmpeg /opt/ffmpeg/bin/ffmpeg
```

An uncertain frame response is retried up to three times with identical bytes
and metadata. A permanent failure after asset creation triggers a best-effort
interrupted seal. There is no cross-process resume or repair; rerunning a
completed standalone upload creates another Project.

## Download original audio

Use the Audio Asset ID shown by `rde project show` to download a sealed asset:

```sh
rde download AUDIO_ASSET_ID
```

The server-provided Unicode filename is used in the current directory. Choose
an exact file path instead when needed:

```sh
rde download AUDIO_ASSET_ID --output recording.webm
```

A clean download is silent and replaces an existing destination. Response
metadata problems produce warnings but do not discard received audio. If the
connection, local write, or command is interrupted, `rde` exits nonzero and
retains received bytes as `FILE.partial` or reports the recognizable temporary
filename that it could retain.

To check a local backend end to end, download the same sealed asset once with
the public endpoint and once with `rde`, then compare the files. `curl` prompts
for the same password stored in `RDE_PASSWORD`, keeping it out of the command
line:

```sh
audio_asset_id=AUDIO_ASSET_ID
check_directory=$(mktemp -d)
curl --fail --user "$RDE_USERNAME" \
  "$RDE_API_URL/api/v1/audio-assets/$audio_asset_id/download" \
  --output "$check_directory/endpoint.webm"
uv run rde download "$audio_asset_id" \
  --output "$check_directory/cli.webm"
cmp "$check_directory/endpoint.webm" "$check_directory/cli.webm"
```

`cmp` exits zero only when both downloads contain exactly the same bytes.

## Develop

```sh
uv run pytest
uv run ruff check .
uv run ruff format --check .
```

Tests invoke the public CLI in subprocesses, use local fake HTTP servers, and
replace `ffmpeg` only at that external process boundary.

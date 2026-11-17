# Audio fixtures

`check-english.wav` is a clean 5.3735-second recording supplied by the repository
owner, who owns the voice and authorized its use in this repository. The audio
player Storybook workbench repeats this recording as its review source.

`check-english.webm` is the voice-oriented Opus derivative used by the local
Storybook HTTP mock. Regenerate it from the WAV with:

```sh
ffmpeg -i check-english.wav -c:a libopus -b:a 24k -application voip \
  -ac 1 -ar 48000 -f webm check-english.webm
```

Source SHA-256:

```text
f86829db362d09c34e3de1bda69f5e2b187afda66053ca2ee1a0d77b3254ccdc  check-english.wav
```
